import QRCode from 'qrcode';
import './style.css';

type Phase='lobby'|'playing'|'finished';
type Player={id:string;name:string;ready:boolean;alive:boolean;connected:boolean;left:boolean;score:number};
type Meaning={category:string;definition:string;reason:string;source:{name:string;url:string}};
type Word={label:string;reading:string;nextStarts:string[];meanings:Meaning[]};
type Room={code:string;phase:Phase;hostId:string;players:Player[];turnPlayerId:string|null;turnId:number;acceptedCount:number;turnDurationMs:number;startsAt:number|null;deadline:number|null;serverNow:number;currentWord:Word|null;history:{playerId:string|null;name:string;word:Word}[];winnerId:string|null;notice:string;canStart:boolean;dictionaryVersion:string};
type Session={code:string;playerId:string;token:string};
const app=document.querySelector<HTMLDivElement>('#app')!;
const icon=(name:string)=>({arrow:'↗',back:'←',link:'↗',check:'✓',close:'×',bolt:'↯',copy:'⧉'}[name]||'→');
const escape=(s:unknown)=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const categories:Record<string,string>={'korean-noun':'국어 명사','lol-champion':'리그 오브 레전드','clash-royale-card':'클래시 로얄','anime-title':'애니메이션','game-title':'게임 제목'};
let room:Room|null=null,session:Session|null=null,ws:WebSocket|null=null;
let connected=false,connecting=false,intentional=false,retry=0,reconnectTimer:number|undefined;
let publicOrigin='',qrCache='',qrCode='',toast='',wordError='',pendingTurn:number|null=null;
let nick=sessionStorage.getItem('wordrelay-name')||'',joinCode=new URLSearchParams(location.search).get('room')?.toUpperCase()||'';
let composing=false,compositionEnded=0,deferredRender=false,lastTurn=-1,hasClock=false,minRtt=Infinity;
let anchor={server:Date.now(),mono:performance.now()};
try {const saved=JSON.parse(sessionStorage.getItem('wordrelay-session')||'null') as Session|null;if(saved&&saved.code===joinCode)session=saved;}catch{/* bad local data is safe to discard */}
function serverNow(){return anchor.server+performance.now()-anchor.mono;}
function notify(message:string){toast=message;const el=document.querySelector('#toast');if(el){el.textContent=message;el.classList.add('visible');}window.setTimeout(()=>{if(toast===message){toast='';document.querySelector('#toast')?.classList.remove('visible');}},4500);}
function send(message:object){if(ws?.readyState===WebSocket.OPEN){ws.send(JSON.stringify(message));return true;}notify('연결을 복구하고 있어요. 잠시만 기다려 주세요.');return false;}
function ping(){if(connected)send({type:'ping',sentAt:Date.now()});}
setInterval(ping,5000);
function connect(request:object){
  if(connecting)return;connecting=true;connected=false;intentional=false;minRtt=Infinity;hasClock=false;render();
  const socket=new WebSocket(`${location.protocol==='https:'?'wss:':'ws:'}//${location.host}/ws`);ws=socket;
  socket.onopen=()=>socket.send(JSON.stringify(request));
  socket.onmessage=({data})=>{
    if(ws!==socket)return;
    let message:any;try{message=JSON.parse(data);}catch{return;}
    if(message.type==='welcome'){
      session={code:message.code,playerId:message.playerId,token:message.token};joinCode=session.code;
      sessionStorage.setItem('wordrelay-session',JSON.stringify(session));
      history.replaceState(null,'',`?room=${encodeURIComponent(joinCode)}`);
      publicOrigin=message.publicOrigin||location.origin;connected=true;connecting=false;retry=0;ping();
    }else if(message.type==='state'){
      room=message.room;connected=true;connecting=false;
      if(!hasClock)anchor={server:room!.serverNow,mono:performance.now()};
      if(room!.turnId!==lastTurn){wordError='';pendingTurn=null;lastTurn=room!.turnId;}
      render();
    }else if(message.type==='pong'){
      const rtt=Date.now()-message.sentAt;
      if(rtt>=0&&rtt<minRtt){minRtt=rtt;anchor={server:message.serverNow+rtt/2,mono:performance.now()};hasClock=true;}
      const latency=document.querySelector('#latency');if(latency)latency.textContent=`${Math.round(Math.max(0,rtt))} ms`;
    }else if(message.type==='error'){
      pendingTurn=null;wordError=message.message;notify(message.message);
      if(['ROOM_NOT_FOUND','ROOM_CLOSED','SESSION_EXPIRED','ROOM_FULL','ALREADY_STARTED','INVALID_NAME','SERVER_BUSY'].includes(message.code)){
        intentional=true;session=null;room=null;sessionStorage.removeItem('wordrelay-session');connected=false;connecting=false;socket.close();
      }render();
    }else if(message.type==='left'){returnHome();}
  };
  socket.onclose=()=>{
    if(ws!==socket)return;connected=false;connecting=false;pendingTurn=null;render();
    if(!intentional&&session){
      const delay=Math.min(500*2**retry++,5000);
      reconnectTimer=window.setTimeout(()=>{if(session)connect({type:'join',code:session.code,token:session.token,name:nick});},delay);
    }else if(!intentional){notify('서버에 연결하지 못했어요. 잠시 뒤 다시 시도해 주세요.');}
  };
  socket.onerror=()=>socket.close();
}
function returnHome(){intentional=true;clearTimeout(reconnectTimer);ws?.close();ws=null;session=null;room=null;connected=false;connecting=false;joinCode='';qrCache='';qrCode='';wordError='';pendingTurn=null;sessionStorage.removeItem('wordrelay-session');history.replaceState(null,'',location.pathname);render();}
function inviteUrl(){const u=new URL(publicOrigin||location.origin);u.searchParams.set('room',room?.code||joinCode);return u.toString();}
async function updateQr(){
  if(!room||room.phase!=='lobby')return;
  const url=inviteUrl();if(qrCode!==url){qrCode=url;qrCache=await QRCode.toDataURL(url,{width:256,margin:2,errorCorrectionLevel:'M',color:{dark:'#19291d',light:'#ffffff'}});}
  const img=document.querySelector<HTMLImageElement>('#qr');if(img&&qrCode===url)img.src=qrCache;
}
function header(){return `<header class="site-header"><a class="brand" href="/" aria-label="이어! 홈"><span class="brand-mark">↔</span>이어<span class="lime-dot">!</span></a><span class="header-caption">생각은 짧게, 말은 길게.</span><div class="connection ${connected?'online':''}"><i></i><span>${room?(connected?'실시간 연결':'다시 연결 중'):'친구와 실시간 끝말잇기'}</span>${connected?'<small id="latency"></small>':''}</div></header>`;}
function footer(){return `<footer><span>이어지는 말, 가까워지는 우리.</span><a href="/attribution.html" target="_blank" rel="noopener">사전 출처</a><span>MADE FOR YOUR NEXT WORD ↗</span></footer><div id="toast" class="toast ${toast?'visible':''}" role="status">${escape(toast)}</div>`;}
function home(){return `<main class="home"><section class="hero"><div class="eyebrow"><span class="pill">2—4 PLAYERS</span> 말 한마디로 시작되는 승부</div><h1>말이 이어질수록,<br>심장은 더 <span class="underlined">빠르게.</span></h1><p class="hero-description">친구를 초대하고, 마지막 글자를 이어 보세요.<br>6초에서 1초까지. 생각할 틈이 점점 줄어듭니다.</p><div class="word-art" aria-hidden="true"><div class="tile t1">사<span>01</span></div><div class="tile t2">과<span>02</span></div><span class="art-arrow">↗</span><div class="tile t3">과<span>03</span></div><div class="tile t4">자<span>04</span></div><div class="mini-timer">↯ 5.9<span>SEC</span></div></div><div class="hero-bottom"><span class="avatars"><b>김</b><b>이</b><b>박</b><b>＋</b></span><span>같은 공간에서도, 멀리 떨어져 있어도.</span></div></section><section class="entry-card"><div class="card-top"><span class="eyebrow">LET’S PLAY</span><span class="corner-arrow">↗</span></div><h2>${joinCode?'친구가 기다리고 있어요.':'우리끼리 한 판?'}</h2><p>가입 없이 이름만 정하면 준비 끝.</p><form id="entry-form"><label for="nickname">어떤 이름으로 불릴까요?</label><input id="nickname" name="nickname" placeholder="닉네임을 입력해 주세요" maxlength="12" autocomplete="nickname" value="${escape(nick)}" required><button class="primary large" type="submit" name="action" value="${joinCode?'join':'create'}" ${connecting?'disabled':''}>${connecting?'연결하는 중…':joinCode?'이 방에 입장하기':'새로운 방 만들기'} <span>＋</span></button><div class="divider"><span>초대받았나요?</span></div><label for="room-code">초대 코드로 입장</label><div class="code-row"><input id="room-code" name="code" value="${escape(joinCode)}" placeholder="6자리 코드" maxlength="6" autocomplete="off" spellcheck="false" aria-label="초대 코드"><button class="dark" name="action" value="join" type="submit" ${connecting?'disabled':''}>입장 <span>→</span></button></div></form><div class="entry-note"><span>◎</span> 친구의 QR 코드를 찍어도 들어올 수 있어요.</div></section><section class="rules-strip"><div><b>01</b><span><strong>모두 준비하면 출발</strong><small>한 방에 최대 4명</small></span></div><div><b>02</b><span><strong>정답마다 0.1초 더 빠르게</strong><small>6.0초 → 최소 1.0초</small></span></div><div><b>03</b><span><strong>마지막까지 살아남기</strong><small>시간 초과 시 탈락</small></span></div></section></main>`;}
function playerCard(p:Player|undefined,i:number){
  if(!p)return `<div class="player-card empty"><span class="avatar">＋</span><strong>친구를 기다려요</strong><span class="seat-label">EMPTY SEAT · 0${i+1}</span></div>`;
  const mine=p.id===session?.playerId;const turn=room?.phase==='playing'&&p.id===room.turnPlayerId;
  const status=p.left?'나감':!p.connected?'연결 복구 중':room?.phase==='lobby'?(p.ready?'준비 완료':'준비 중'):!p.alive?'탈락 · 관전 중':turn?'지금 차례':'대기 중';
  return `<div class="player-card color-${i} ${p.ready&&room?.phase==='lobby'?'ready':''} ${turn?'current':''} ${!p.alive?'out':''}"><div class="player-top"><span class="seat-label">PLAYER 0${i+1}</span>${p.id===room?.hostId?'<span class="host-tag">방장</span>':''}</div><span class="avatar">${escape(p.name.slice(0,1))}</span><strong>${escape(p.name)} ${mine?'<em>나</em>':''}</strong><span class="player-status">${p.ready&&room?.phase==='lobby'?'✓ ':''}${status}</span>${room?.phase!=='lobby'?`<span class="score">${p.score}<small> 단어</small></span>`:''}</div>`;
}
function lobby(){const r=room!;const me=r.players.find(p=>p.id===session?.playerId);const host=r.hostId===session?.playerId;
 return `<main class="room-page"><div class="room-toolbar"><button class="text-button" data-action="leave">← 나가기</button><span class="room-code">ROOM <b>${r.code}</b><button aria-label="초대 코드 복사" data-action="copy-code">⧉</button></span></div><div class="lobby-layout"><section class="lobby-main"><div class="eyebrow">WAITING ROOM <span class="live-dot"></span></div><h1>함께할 준비,<br><span class="underlined">됐나요?</span></h1><p class="muted">모두 준비되면 방장이 게임을 시작할 수 있어요.</p><div class="section-title"><h3>플레이어 <span>${r.players.length} / 4</span></h3><span>입장 후 준비를 눌러 주세요</span></div><div class="players lobby-players">${Array.from({length:4},(_,i)=>playerCard(r.players[i],i)).join('')}</div><div class="lobby-actions"><button class="${me?.ready?'ready-button':'dark'}" data-action="ready" ${!connected?'disabled':''}>${me?.ready?'✓ 준비 완료 · 취소하기':'준비하기'}</button>${host?`<button class="primary" data-action="start" ${!r.canStart||!connected?'disabled':''}>게임 시작 <span>→</span></button>`:`<span class="waiting-label">방장이 시작하기를 기다리고 있어요.</span>`}</div><p class="small-note">최소 2명 · 모두 준비 · 제한 시간 6.0 → 1.0초</p></section><aside class="invite-card"><div class="eyebrow">BRING YOUR FRIENDS</div><h2>여기로 모여요.</h2><p>카메라로 QR 코드를 찍으면<br>이 방으로 바로 연결돼요.</p><div class="qr-frame"><img id="qr" width="200" height="200" alt="이 대기방에 입장하는 QR 코드" ${qrCache?`src="${qrCache}"`:''}><span class="qr-center-label">SCAN & JOIN</span></div><div class="invite-code">${r.code}</div><button class="outline" data-action="copy-link">초대 링크 복사 <span>⧉</span></button><input class="invite-url" readonly aria-label="초대 주소" value="${escape(inviteUrl())}">${['localhost','127.0.0.1'].includes(location.hostname)?'<p class="local-note">휴대폰 초대는 같은 Wi-Fi의 컴퓨터 IP 주소나 배포 주소로 접속해 주세요.</p>':''}<div class="invite-foot">최대 4명, 재미는 그 이상.</div></aside></div></main>`;
}
function wordMarkup(w:Word){const chars=Array.from(w.label);const last=chars.pop()||'';return `<span>${escape(chars.join(''))}</span><mark>${escape(last)}</mark>`;}
function meanings(w:Word){return `<details class="dictionary-panel"><summary><span class="dictionary-icon">가</span><span><strong>왜 정답인가요?</strong><small>${escape(w.label)} · 뜻과 인정 근거</small></span><span class="expand-icon">＋</span></summary><div class="meaning-list">${w.meanings.map(m=>`<article><span class="category">${escape(categories[m.category]||m.category)}</span><p>${escape(m.definition)}</p><small>${escape(m.reason)}</small><a href="${escape(m.source.url)}" target="_blank" rel="noopener noreferrer">${escape(m.source.name)} ↗</a></article>`).join('')}</div></details>`;}
function playing(){const r=room!;const me=r.players.find(p=>p.id===session?.playerId);const mine=r.turnPlayerId===me?.id;const current=r.players.find(p=>p.id===r.turnPlayerId);const w=r.currentWord!;
return `<main class="game-page"><div class="room-toolbar"><button class="text-button" data-action="leave">← 나가기</button><span class="room-code">ROOM <b>${r.code}</b></span><span class="round-label">${r.acceptedCount} WORDS CONNECTED</span></div><div class="players game-players" style="--player-count:${r.players.length}">${r.players.map(playerCard).join('')}</div><section class="arena"><div class="arena-top"><span class="pill ${mine?'lime':''}" id="turn-label">${mine?'지금 내 차례':`${escape(current?.name)} 님의 차례`}</span><span class="turn-meta">정답마다 −0.1초</span></div><div class="timer"><span class="timer-label" id="timer-label">남은 시간</span><strong id="time-value" role="timer">${(r.turnDurationMs/1000).toFixed(1)}</strong><span class="seconds">SEC</span></div><div class="time-track"><div id="time-progress"></div></div><div class="current-word" data-testid="current-word">${wordMarkup(w)}</div><div class="word-prompt"><b>${escape(w.nextStarts.join(' / '))}</b> ${mine?'로 시작하는 단어를 입력하세요.':'로 이어질 다음 단어는?'}</div><form id="word-form" class="word-form"><label class="sr-only" for="word-input">끝말잇기 단어</label><input id="word-input" maxlength="256" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="${!me?.alive?'탈락했어요. 친구들의 승부를 지켜보세요.':mine?'생각났다면, 바로 입력!':'다음 차례를 기다려 주세요.'}" ${!mine||!me?.alive||!connected?'disabled':''}><button class="primary" type="submit" id="submit-word" ${!mine||!connected?'disabled':''}>입력 <kbd>↵</kbd></button></form><p class="word-feedback ${wordError?'error':''}" role="status">${escape(wordError||r.notice||(mine?'국어 · 게임 · 애니메이션 이름, 모두 가능해요.':'준비한 단어를 마음속으로 떠올려 보세요.'))}</p></section><div class="game-bottom"><section class="history-panel"><div class="section-title"><h3>이어온 단어</h3><span>${r.acceptedCount}개 성공</span></div><div class="word-history">${r.history.map((h,i)=>`<div class="history-item ${i===r.history.length-1?'latest':''}"><small>${escape(h.name)}</small><strong>${escape(h.word.label)}</strong></div>`).join('<span class="history-arrow">→</span>')}</div></section>${meanings(w)}</div></main>`;}
function finished(){const r=room!;const winner=r.players.find(p=>p.id===r.winnerId);const mine=winner?.id===session?.playerId;
return `<main class="results-page"><div class="room-toolbar"><button class="text-button" data-action="leave">← 나가기</button><span class="room-code">ROOM <b>${r.code}</b></span></div><section class="result-card"><span class="eyebrow">THAT’S A GOOD GAME</span><div class="winner-symbol">${winner?'✳':'↔'}</div><h1>${winner?`${escape(winner.name)} 님의<br><span class="underlined">멋진 한 판!</span>`:'다음 판에<br>다시 만나요.'}</h1><p>${mine?'마지막까지 이어냈어요. 오늘의 승자는 나!':escape(r.notice)}</p><div class="result-stats"><div><strong>${r.acceptedCount}</strong><span>이어온 단어</span></div><div><strong>${(r.turnDurationMs/1000).toFixed(1)}<small>초</small></strong><span>마지막 제한 시간</span></div></div><div class="result-players">${[...r.players].sort((a,b)=>Number(b.id===r.winnerId)-Number(a.id===r.winnerId)||b.score-a.score).map((p,i)=>`<div><span>${p.id===r.winnerId?'★':String(i+1).padStart(2,'0')}</span><strong>${escape(p.name)}${p.id===session?.playerId?' <em>나</em>':''}</strong><b>${p.score} <small>단어</small></b></div>`).join('')}</div>${r.hostId===session?.playerId?'<button class="primary large" data-action="rematch">한 판 더 준비하기 <span>↗</span></button>':'<p class="small-note">방장이 다시 준비하면 다음 판을 시작할 수 있어요.</p>'}<button class="text-button" data-action="leave">처음으로 돌아가기</button></section></main>`;
}
function render(){
  if(composing){deferredRender=true;return;}
  const focused=document.activeElement as HTMLInputElement|null;
  const focusId=focused?.id;const value=focused?.value;const start=focused?.selectionStart;
  const detailsOpen=document.querySelector<HTMLDetailsElement>('.dictionary-panel')?.open;
  const oldTurn=app.dataset.turn;app.dataset.turn=String(room?.turnId??'');
  app.innerHTML=header()+(room?(room.phase==='lobby'?lobby():room.phase==='playing'?playing():finished()):home())+footer();
  if(detailsOpen){const details=document.querySelector<HTMLDetailsElement>('.dictionary-panel');if(details)details.open=true;}
  if(focusId){const next=document.getElementById(focusId) as HTMLInputElement|null;
    if(next&&!next.disabled&&(focusId!=='word-input'||oldTurn===app.dataset.turn)){if(value!==undefined)next.value=value;next.focus({preventScroll:true});if(start!==null&&start!==undefined)next.setSelectionRange(start,start);}}
  if(room?.phase==='playing'&&oldTurn!==app.dataset.turn&&room.turnPlayerId===session?.playerId){document.querySelector<HTMLInputElement>('#word-input')?.focus({preventScroll:true});}
  void updateQr().catch(()=>notify('QR 코드를 만들지 못했어요. 초대 링크로 입장해 주세요.'));
  paintTimer();
}
app.addEventListener('input',event=>{const el=event.target as HTMLInputElement;if(el.id==='nickname'){nick=el.value;sessionStorage.setItem('wordrelay-name',nick);}if(el.id==='room-code'){joinCode=el.value.toUpperCase().replace(/[^A-Z0-9]/g,'');el.value=joinCode;}});
app.addEventListener('compositionstart',()=>{composing=true;});
app.addEventListener('compositionend',()=>{composing=false;compositionEnded=performance.now();if(deferredRender){deferredRender=false;render();}});
app.addEventListener('submit',event=>{
 event.preventDefault();const form=event.target as HTMLFormElement;
 if(form.id==='entry-form'){
  nick=(document.querySelector<HTMLInputElement>('#nickname')?.value||'').trim();if(!nick||Array.from(nick).length>12){notify('이름을 1~12글자로 입력해 주세요.');return;}
  const action=(event as SubmitEvent).submitter?.getAttribute('value')||'create';
  if(action==='join'&&!/^[A-Z0-9]{6}$/.test(joinCode)){notify('6자리 초대 코드를 입력해 주세요.');return;}
  sessionStorage.setItem('wordrelay-name',nick);connect(action==='join'?{type:'join',code:joinCode,name:nick}:{type:'create',name:nick});
 }else if(form.id==='word-form'){
  if(composing||performance.now()-compositionEnded<70)return;
  if(!room||pendingTurn!==null||!connected||room.turnPlayerId!==session?.playerId||serverNow()<(room.startsAt||0)||serverNow()>=(room.deadline||0))return;
  const input=document.querySelector<HTMLInputElement>('#word-input')!;const word=input.value.trim();if(!word)return;
  pendingTurn=room.turnId;wordError='';send({type:'submit',word,turnId:room.turnId});paintTimer();
 }
});
app.addEventListener('click',async event=>{
 const target=(event.target as HTMLElement).closest<HTMLElement>('[data-action]');if(!target)return;
 const action=target.dataset.action;
 if(action==='ready'){const me=room?.players.find(p=>p.id===session?.playerId);send({type:'ready',ready:!me?.ready});}
 if(action==='start')send({type:'start'});
 if(action==='rematch')send({type:'rematch'});
 if(action==='leave'){if(room?.phase==='playing'&&!window.confirm('진행 중인 게임에서 나가면 탈락합니다. 나갈까요?'))return;if(connected)send({type:'leave'});else returnHome();}
 if(action==='copy-code'||action==='copy-link'){
   try{await navigator.clipboard.writeText(action==='copy-code'?room!.code:inviteUrl());notify(action==='copy-code'?'초대 코드를 복사했어요.':'초대 링크를 복사했어요. 친구에게 보내 주세요.');}
   catch{document.querySelector<HTMLInputElement>('.invite-url')?.select();notify('아래 초대 주소를 길게 눌러 복사해 주세요.');}
 }
});
let enabledBefore=false;
function paintTimer(){
 if(room?.phase!=='playing')return;
 const now=serverNow(),countdown=Math.max(0,(room.startsAt||0)-now),left=Math.max(0,(room.deadline||0)-now);
 const time=document.querySelector('#time-value');if(time)time.textContent=countdown>0?String(Math.ceil(countdown/1000)):(Math.min(left,room.turnDurationMs)/1000).toFixed(1);
 const label=document.querySelector('#timer-label');if(label)label.textContent=countdown>0?'곧 시작해요':'남은 시간';
 const progress=document.querySelector<HTMLElement>('#time-progress');if(progress)progress.style.transform=`scaleX(${countdown>0?1:Math.min(1,left/room.turnDurationMs)})`;
 document.querySelector('.timer')?.classList.toggle('urgent',countdown===0&&left<=1500);
 const enabled=connected&&room.turnPlayerId===session?.playerId&&countdown===0&&left>0&&pendingTurn===null;
 const input=document.querySelector<HTMLInputElement>('#word-input');const button=document.querySelector<HTMLButtonElement>('#submit-word');
 if(input)input.disabled=!enabled;if(button)button.disabled=!enabled;
 if(enabled&&!enabledBefore)input?.focus({preventScroll:true});enabledBefore=enabled;
}
function animate(){paintTimer();requestAnimationFrame(animate);}requestAnimationFrame(animate);
window.addEventListener('online',()=>{if(!connected&&!connecting&&session){clearTimeout(reconnectTimer);connect({type:'join',code:session.code,token:session.token,name:nick});}});
render();if(session)connect({type:'join',code:session.code,token:session.token,name:nick});
