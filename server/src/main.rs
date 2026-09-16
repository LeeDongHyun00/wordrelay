mod dictionary;
mod game;

use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use dictionary::Dictionary;
use futures_util::{SinkExt, StreamExt};
use game::{Game, Phase};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{RwLock, Semaphore, mpsc, oneshot};
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};
use uuid::Uuid;

type Outbox = mpsc::Sender<Arc<str>>;
type RoomTx = mpsc::Sender<Command>;
struct App {
    dict: Arc<Dictionary>,
    rooms: RwLock<HashMap<String, RoomTx>>,
    sockets: Arc<Semaphore>,
    public_origin: Option<String>,
}
#[derive(Deserialize, Debug)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum ClientMessage {
    Create {
        name: String,
    },
    Join {
        code: String,
        name: String,
        token: Option<String>,
    },
    Ready {
        ready: bool,
    },
    Configure {
        rounds: u32,
    },
    Start,
    Submit {
        word: String,
        turn_id: u64,
    },
    Kick {
        player_id: String,
    },
    Rematch,
    Leave,
    Ping {
        sent_at: f64,
    },
}
enum Command {
    Attach {
        name: String,
        token: Option<String>,
        generation: Uuid,
        out: Outbox,
        reply: oneshot::Sender<Option<String>>,
    },
    Action {
        id: String,
        generation: Uuid,
        msg: ClientMessage,
    },
    Disconnect {
        id: String,
        generation: Uuid,
        expired: bool,
    },
}
struct Client {
    generation: Uuid,
    out: Outbox,
}
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn event(v: serde_json::Value) -> Arc<str> {
    Arc::from(v.to_string())
}
fn error(out: &Outbox, code: &str, message: &str) {
    let _ = out.try_send(event(json!({"type":"error","code":code,"message":message})));
}
fn publish(game: &mut Game, clients: &mut HashMap<String, Client>, d: &Dictionary) {
    let payload = event(json!({"type":"state","room":game.snapshot(d,Instant::now(),unix_ms())}));
    let failed: Vec<_> = clients
        .iter()
        .filter_map(|(id, c)| c.out.try_send(payload.clone()).err().map(|_| id.clone()))
        .collect();
    for id in &failed {
        clients.remove(id);
        game.disconnect(id, false, d, Instant::now());
    }
    if !failed.is_empty() {
        let payload =
            event(json!({"type":"state","room":game.snapshot(d,Instant::now(),unix_ms())}));
        for c in clients.values() {
            let _ = c.out.try_send(payload.clone());
        }
    }
}
async fn run_room(app: Arc<App>, code: String, mut rx: mpsc::Receiver<Command>) {
    let mut game = Game::new(code.clone(), Instant::now());
    let mut clients: HashMap<String, Client> = HashMap::new();
    let mut sweep = tokio::time::interval(Duration::from_secs(1));
    loop {
        let deadline = game
            .deadline
            .or(game.next_round_at)
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            biased;
            _=tokio::time::sleep_until(deadline.into()), if matches!(game.phase,Phase::Playing|Phase::Intermission) => {
                if game.timeout(&app.dict,Instant::now()) || game.advance_round(&app.dict,Instant::now()) {publish(&mut game,&mut clients,&app.dict);}
            }
            command=rx.recv()=> {
                let Some(command)=command else {break};
                match command {
                    Command::Attach{name,token,generation,out,reply}=>{
                        match game.attach(&name,token.as_deref(),Instant::now()) {
                            Ok(i)=>{
                                let p=&game.players[i];let id=p.id.clone();
                                let welcome=event(json!({"type":"welcome","playerId":id,"token":p.token,"code":code,"publicOrigin":app.public_origin}));
                                let _=out.try_send(welcome);clients.insert(id.clone(),Client{generation,out});
                                if reply.send(Some(id.clone())).is_err() {clients.remove(&id);game.disconnect(&id,false,&app.dict,Instant::now());}
                                publish(&mut game,&mut clients,&app.dict);
                            }
                            Err(e)=>{error(&out,e.code,&e.message);let _=reply.send(None);}
                        }
                    }
                    Command::Action{id,generation,msg}=>{
                        if !clients.get(&id).is_some_and(|c|c.generation==generation) {continue}
                        let now=Instant::now();
                        let turn_before=game.turn_id;
                        let result=match msg {
                            ClientMessage::Ready{ready}=>game.ready(&id,ready,now),
                            ClientMessage::Configure{rounds}=>game.configure(&id,rounds,now),
                            ClientMessage::Start=>game.start(&id,&app.dict,now),
                            ClientMessage::Submit{word,turn_id}=>game.submit(&id,turn_id,&word,&app.dict,now),
                            ClientMessage::Kick{player_id}=>{
                                let result=game.kick(&id,&player_id,&app.dict,now);
                                if result.is_ok() && let Some(c)=clients.remove(&player_id) {
                                    let _=c.out.try_send(event(json!({"type":"kicked","message":"방장에 의해 강퇴되었습니다."})));
                                }
                                result
                            }
                            ClientMessage::Rematch=>game.rematch(&id,now),
                            ClientMessage::Leave=>{
                                if let Some(c)=clients.remove(&id) {let _=c.out.try_send(event(json!({"type":"left"})));}
                                game.disconnect(&id,true,&app.dict,now);Ok(())
                            }
                            _=>Err(game::err("BAD_ACTION","지원하지 않는 요청입니다.")),
                        };
                        let changed=result.is_ok() || game.turn_id != turn_before;
                        if let Err(e)=result && let Some(c)=clients.get(&id) {error(&c.out,e.code,&e.message);}
                        if changed {publish(&mut game,&mut clients,&app.dict);}
                    }
                    Command::Disconnect{id,generation,expired}=>{
                        if clients.get(&id).is_some_and(|c|c.generation==generation) {
                            clients.remove(&id);game.disconnect(&id,expired,&app.dict,Instant::now());publish(&mut game,&mut clients,&app.dict);
                        }
                    }
                }
            }
            _=sweep.tick()=>{
                let now=Instant::now();
                if game.sweep(&app.dict,now) {publish(&mut game,&mut clients,&app.dict);}
                let idle=now.duration_since(game.updated);
                if (clients.is_empty() && idle>Duration::from_secs(60)) || idle>Duration::from_secs(1800) {break}
            }
        }
    }
    for c in clients.values() {
        error(
            &c.out,
            "ROOM_CLOSED",
            "방이 종료되었습니다. 새 방에서 만나요.",
        );
    }
    app.rooms.write().await.remove(&code);
}
async fn get_room(
    app: &Arc<App>,
    request: &ClientMessage,
) -> Result<RoomTx, (&'static str, &'static str)> {
    match request {
        ClientMessage::Create { .. } => {
            let mut rooms = app.rooms.write().await;
            if rooms.len() >= 128 {
                return Err((
                    "SERVER_BUSY",
                    "열린 방이 많아요. 잠시 뒤 다시 시도해 주세요.",
                ));
            }
            const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
            let code = loop {
                let n = Uuid::new_v4().as_u128();
                let code: String = (0..6)
                    .map(|i| ALPHABET[((n >> (i * 5)) % ALPHABET.len() as u128) as usize] as char)
                    .collect();
                if !rooms.contains_key(&code) {
                    break code;
                }
            };
            let (tx, rx) = mpsc::channel(128);
            rooms.insert(code.clone(), tx.clone());
            tokio::spawn(run_room(app.clone(), code, rx));
            Ok(tx)
        }
        ClientMessage::Join { code, .. } => app
            .rooms
            .read()
            .await
            .get(&code.to_uppercase())
            .cloned()
            .ok_or((
                "ROOM_NOT_FOUND",
                "방을 찾을 수 없어요. 초대 코드를 확인해 주세요.",
            )),
        _ => Err(("HANDSHAKE_REQUIRED", "먼저 방에 입장해 주세요.")),
    }
}
async fn send(socket: &mut WebSocket, value: serde_json::Value) {
    let _ = socket.send(Message::Text(value.to_string().into())).await;
}
async fn socket_session(
    mut socket: WebSocket,
    app: Arc<App>,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let initial = tokio::time::timeout(Duration::from_secs(8), socket.recv()).await;
    let Ok(Some(Ok(Message::Text(raw)))) = initial else {
        return;
    };
    let Ok(request) = serde_json::from_str::<ClientMessage>(&raw) else {
        send(
            &mut socket,
            json!({"type":"error","code":"BAD_MESSAGE","message":"잘못된 입장 요청입니다."}),
        )
        .await;
        return;
    };
    let (name, token) = match &request {
        ClientMessage::Create { name } => (name.clone(), None),
        ClientMessage::Join { name, token, .. } => (name.clone(), token.clone()),
        _ => return,
    };
    let room = match get_room(&app, &request).await {
        Ok(tx) => tx,
        Err((code, message)) => {
            send(
                &mut socket,
                json!({"type":"error","code":code,"message":message}),
            )
            .await;
            return;
        }
    };
    let (out, mut incoming) = mpsc::channel(32);
    let (reply, wait) = oneshot::channel();
    let generation = Uuid::new_v4();
    if room
        .send(Command::Attach {
            name,
            token,
            generation,
            out: out.clone(),
            reply,
        })
        .await
        .is_err()
    {
        return;
    }
    let Ok(Some(id)) = wait.await else {
        if let Some(payload) = incoming.recv().await {
            let _ = socket.send(Message::Text(payload.to_string().into())).await;
        }
        return;
    };
    drop(out);
    let (mut writer, mut reader) = socket.split();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
    let mut expired = false;
    let mut last_seen = Instant::now();
    let mut window = Instant::now();
    let mut messages = 0;
    loop {
        tokio::select! {
            payload=incoming.recv()=>{
                let Some(payload)=payload else {break};
                if !matches!(tokio::time::timeout(Duration::from_secs(3),writer.send(Message::Text(payload.to_string().into()))).await,Ok(Ok(()))) {break}
            }
            frame=reader.next()=>{
                let Some(Ok(frame))=frame else {break};
                match frame {
                    Message::Text(raw)=>{
                        last_seen=Instant::now();
                        if window.elapsed()>=Duration::from_secs(1) {window=Instant::now();messages=0;}
                        messages+=1;if messages>25 {let _=writer.send(Message::Close(None)).await;break}
                        let Ok(msg)=serde_json::from_str::<ClientMessage>(&raw) else {let _=writer.send(Message::Text(json!({"type":"error","code":"BAD_MESSAGE","message":"요청 형식이 올바르지 않습니다."}).to_string().into())).await;continue};
                        if let ClientMessage::Ping{sent_at}=msg {
                            let _=writer.send(Message::Text(json!({"type":"pong","sentAt":sent_at,"serverNow":unix_ms()}).to_string().into())).await;
                        } else if room.try_send(Command::Action{id:id.clone(),generation,msg}).is_err() {break}
                    }
                    Message::Close(_)=>break,
                    Message::Ping(bytes)=>{let _=writer.send(Message::Pong(bytes)).await;}
                    _=>{}
                }
            }
            _=heartbeat.tick()=>{
                if last_seen.elapsed()>=game::RECONNECT_GRACE {expired=true;break}
                if writer.send(Message::Ping(vec![].into())).await.is_err() {break}
            }
        }
    }
    let _ = room
        .send(Command::Disconnect {
            id,
            generation,
            expired,
        })
        .await;
}
async fn ws(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    // Same-origin by default; behind a proxy set PUBLIC_ORIGIN to the public HTTPS origin.
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|x| x.to_str().ok()) {
        let permitted = if let Some(expected) = &app.public_origin {
            origin.trim_end_matches('/') == expected.trim_end_matches('/')
        } else {
            headers
                .get(header::HOST)
                .and_then(|h| h.to_str().ok())
                .is_some_and(|host| {
                    origin == format!("http://{host}") || origin == format!("https://{host}")
                })
        };
        if !permitted {
            return (StatusCode::FORBIDDEN, "Origin not allowed").into_response();
        }
    }
    let Ok(permit) = app.sockets.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    upgrade
        .max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| socket_session(socket, app, permit))
        .into_response()
}
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "wordrelay=info".into()),
        )
        .init();
    tracing::info!("starting dictionary load");
    let dict = Arc::new(
        Dictionary::load(
            &std::env::var("DICTIONARY_PATH").unwrap_or("data/dictionary.json".into()),
        )
        .expect("정답 사전을 읽을 수 없습니다"),
    );
    tracing::info!(words=dict.entries.len(),version=%dict.version,"dictionary loaded");
    let public_origin = std::env::var("PUBLIC_ORIGIN")
        .or_else(|_| std::env::var("RENDER_EXTERNAL_URL"))
        .ok()
        .map(|s| s.trim_end_matches('/').to_owned());
    let app = Arc::new(App {
        dict,
        rooms: RwLock::new(HashMap::new()),
        sockets: Arc::new(Semaphore::new(1024)),
        public_origin,
    });
    let static_dir = std::env::var("STATIC_DIR").unwrap_or("dist".into());
    let router = Router::new()
        .route("/ws", get(ws))
        .route("/api/health", get(|| async { Json(json!({"ok":true})) }))
        .fallback_service(
            ServeDir::new(&static_dir)
                .not_found_service(ServeFile::new(format!("{static_dir}/index.html"))),
        )
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            axum::http::HeaderValue::from_static("nosniff"),
        ))
        .with_state(app);
    let addr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| format!("0.0.0.0:{}", std::env::var("PORT").unwrap_or("3000".into())));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("서버 포트를 열 수 없습니다");
    tracing::info!(%addr,"wordrelay ready");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .unwrap();
}
