import QRCode from 'qrcode';
import './style.css';
import {displayTime,timerStage} from './timing';
import {enterView,turnMotion,feedbackMotion,changedPlayer,disclosure} from './motion';

type Phase='lobby'|'playing'|'intermission'|'finished';
type Player={id:string;name:string;ready:boolean;alive:boolean;connected:boolean;left:boolean;score:number;roundScore:number;roundPenalty:number};
type Meaning={category:string;definition:string;reason:string;source:{name:string;url:string}};
type Word={label:string;reading:string;nextStarts:string[];meanings:Meaning[]};
type Room={code:string;phase:Phase;hostId:string;players:Player[];turnPlayerId:string|null;turnId:number;acceptedCount:number;turnDurationMs:number;startsAt:number|null;deadline:number|null;serverNow:number;currentWord:Word|null;history:{playerId:string|null;name:string;word:Word;points:number}[];totalRounds:number;round:number;nextRoundAt:number|null;winnerIds:string[];roundResults:{round:number;winnerId:string|null;winnerIds:string[];failedId:string|null;scores:{playerId:string;points:number;total:number;penalty:number}[]}[];winnerId:string|null;notice:string;canStart:boolean;dictionaryVersion:string};
type Session={code:string;playerId:string;token:string};
const app=document.querySelector<HTMLDivElement>('#app')!;
const icon=(name:string)=>({arrow:'↗',back:'←',link:'↗',check:'✓',close:'×',bolt:'↯',copy:'⧉'}[name]||'→');
const escape=(s:unknown)=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const categories:Record<string,string>={'korean-noun':'국어 명사','lol-champion':'리그 오브 레전드','clash-royale-card':'클래시 로얄','anime-title':'애니메이션','game-title':'게임 제목','station-name':'역 이름'};
let room:Room|null=null,session:Session|null=null,ws:WebSocket|null=null;
let connected=false,connecting=false,intentional=false,retry=0,reconnectTimer:number|undefined;
let recoveryTimer:number|undefined;
let soundEnabled=false,audioContext:AudioContext|undefined,lastAlertTurn=-1;
function turnAlert(){if(!room||room.phase!=='playing'||room.turnPlayerId!==session?.playerId||serverNow()<(room.startsAt||0)||lastAlertTurn===room.turnId)return;lastAlertTurn=room.turnId;document.title='내 차례 — 이어';navigator.vibrate?.([90,50,90]);if(soundEnabled&&audioContext?.state==='running'){const tone=audioContext.createOscillator(),gain=audioContext.createGain();tone.frequency.value=880;gain.gain.setValueAtTime(.08,audioContext.currentTime);gain.gain.exponentialRampToValueAtTime(.001,audioContext.currentTime+.18);tone.connect(gain);gain.connect(audioContext.destination);tone.start();tone.stop(audioContext.currentTime+.18);}}
let pendingMotion:'success'|'error'|null=null, motionPoints=0;
let publicOrigin='',qrCache='',qrCode='',toast='',wordError='',wordDraft='',pendingTurn:number|null=null;
let nick=sessionStorage.getItem('wordrelay-name')||'',joinCode=new URLSearchParams(location.search).get('room')?.toUpperCase()||'';
let composing=false,submitAfterComposition=false,deferredRender=false,lastTurn=-1,hasClock=false,minRtt=Infinity;
let anchor={server:Date.now(),mono:performance.now()};
try {const saved=JSON.parse(sessionStorage.getItem('wordrelay-session')||'null') as Session|null;if(saved&&saved.code===joinCode)session=saved;}catch{/* bad local data is safe to discard */}
function serverNow(){return anchor.server+performance.now()-anchor.mono;}
function notify(message:string){toast=message;const el=document.querySelector('#toast');if(el){el.textContent=message;el.classList.add('visible');}window.setTimeout(()=>{if(toast===message){toast='';document.querySelector('#toast')?.classList.remove('visible');}},4500);}
function send(message:object){if(ws?.readyState===WebSocket.OPEN){ws.send(JSON.stringify(message));return true;}notify('연결을 복구하고 있어요. 잠시만 기다려 주세요.');return false;}
function ping(){if(connected)send({type:'ping',sentAt:Date.now()});}
setInterval(ping,1000);
function connect(request:object){
  if(connecting)return;connecting=true;connected=false;intentional=false;minRtt=Infinity;hasClock=false;render();
  const socket=new WebSocket(`${location.protocol==='https:'?'wss:':'ws:'}//${location.host}/ws`);ws=socket;
  socket.onopen=()=>socket.send(JSON.stringify(request));
  socket.onmessage=({data})=>{
    if(ws!==socket)return;
    let message:any;try{message=JSON.parse(data);}catch{return;}
    if(message.type==='welcome'){
      if(session?.code!==message.code)lastAlertTurn=-1;
      clearTimeout(recoveryTimer);recoveryTimer=undefined;
      session={code:message.code,playerId:message.playerId,token:message.token};joinCode=session.code;
      sessionStorage.setItem('wordrelay-session',JSON.stringify(session));
      history.replaceState(null,'',`?room=${encodeURIComponent(joinCode)}`);
      publicOrigin=message.publicOrigin||location.origin;connected=true;connecting=false;retry=0;ping();
    }else if(message.type==='state'){
      const previous=room;
      room=message.room;connected=true;connecting=false;
      if(previous?.code===room!.code&&previous.round===room!.round&&room!.acceptedCount>previous.acceptedCount){pendingMotion='success';motionPoints=room!.history.at(-1)?.points||0;}

      if(!hasClock)anchor={server:room!.serverNow,mono:performance.now()};
      if(room!.turnId!==lastTurn){wordError='';wordDraft='';pendingTurn=null;lastTurn=room!.turnId;}
      document.title=room?.turnPlayerId===session?.playerId?'내 차례 — 이어':'이어 — 실시간 끝말잇기';
      render();
    }else if(message.type==='pong'){
      const rtt=Date.now()-message.sentAt;
      if(rtt>=0&&rtt<minRtt){minRtt=rtt;anchor={server:message.serverNow+rtt/2,mono:performance.now()};hasClock=true;}
    }else if(message.type==='error'){
      pendingTurn=null;wordError=message.message;pendingMotion='error';if(room?.phase!=='playing')notify(message.message);
      if(['ROOM_NOT_FOUND','ROOM_CLOSED','SESSION_EXPIRED','ROOM_FULL','ALREADY_STARTED','INVALID_NAME','SERVER_BUSY'].includes(message.code)){
        intentional=true;session=null;room=null;sessionStorage.removeItem('wordrelay-session');connected=false;connecting=false;socket.close();
      }render();
      if(room?.phase==='playing')document.querySelector<HTMLInputElement>('#word-input')?.select();
    }else if(message.type==='kicked'){returnHome();notify(message.message);}else if(message.type==='left'){returnHome();}
  };
  socket.onclose=()=>{
    if(ws!==socket)return;connected=false;connecting=false;pendingTurn=null;render();
    if(!intentional&&session){
      if(recoveryTimer===undefined)recoveryTimer=window.setTimeout(()=>{returnHome();notify('6초 동안 연결되지 않아 퇴장했습니다.');},6000);
      const delay=Math.min(500*2**retry++,5000);
      reconnectTimer=window.setTimeout(()=>{if(session)connect({type:'join',code:session.code,token:session.token,name:nick});},delay);
    }else if(!intentional){notify('서버에 연결하지 못했어요. 잠시 뒤 다시 시도해 주세요.');}
  };
  socket.onerror=()=>socket.close();
}
function returnHome(){pendingMotion=null;lastAlertTurn=-1;clearTimeout(recoveryTimer);recoveryTimer=undefined;document.title='이어 — 실시간 끝말잇기';intentional=true;clearTimeout(reconnectTimer);ws?.close();ws=null;session=null;room=null;connected=false;connecting=false;joinCode='';qrCache='';qrCode='';wordError='';wordDraft='';composing=false;submitAfterComposition=false;pendingTurn=null;sessionStorage.removeItem('wordrelay-session');history.replaceState(null,'',location.pathname);render();}
function inviteUrl(){const u=new URL(publicOrigin||location.origin);u.searchParams.set('room',room?.code||joinCode);return u.toString();}
async function updateQr(){
  if(!room||room.phase!=='lobby')return;
  const url=inviteUrl();if(qrCode!==url){qrCode=url;qrCache=await QRCode.toDataURL(url,{width:256,margin:2,errorCorrectionLevel:'M',color:{dark:'#19291d',light:'#ffffff'}});}
  const img=document.querySelector<HTMLImageElement>('#qr');if(img&&qrCode===url)img.src=qrCache;
}
function header(){return `<header class="site-header"><a class="brand" href="/" aria-label="이어 홈"><span class="brand-mark" aria-hidden="true">↔</span>이어</a>${room?`<div class="connection ${connected?'online':''}"><i></i><span>${connected?'연결됨':'재접속 중'}</span></div>`:''}</header>`;}
function footer(){return `<footer><a href="/attribution.html" target="_blank" rel="noopener">사전 출처</a></footer><div id="toast" class="toast ${toast?'visible':''}" role="status">${escape(toast)}</div>`;}
function home(){return `<main class="home"><section class="hero"><div class="eyebrow"><span class="pill">2–4명</span></div><h1>끝말잇기</h1><div class="word-art" aria-hidden="true"><div class="tile t1">사<span>01</span></div><div class="tile t2">과<span>02</span></div><span class="art-arrow">↗</span><div class="tile t3">과<span>03</span></div><div class="tile t4">자<span>04</span></div><div class="mini-timer">↯ 5.9<span>SEC</span></div></div></section><section class="entry-card"><h2>${joinCode?'방 입장':'방 만들기'}</h2><form id="entry-form"><label for="nickname">닉네임</label><input id="nickname" name="nickname" placeholder="닉네임을 입력해 주세요" maxlength="12" autocomplete="nickname" value="${escape(nick)}" required><button class="primary large" type="submit" name="action" value="${joinCode?'join':'create'}" ${connecting?'disabled':''}>${connecting?'연결하는 중…':joinCode?'이 방에 입장하기':'새로운 방 만들기'} <span>＋</span></button><div class="divider"><span>또는</span></div><label for="room-code">초대 코드로 입장</label><div class="code-row"><input id="room-code" name="code" value="${escape(joinCode)}" placeholder="6자리 코드" maxlength="6" autocomplete="off" spellcheck="false" aria-label="초대 코드"><button class="dark" name="action" value="join" type="submit" ${connecting?'disabled':''}>입장 <span>→</span></button></div></form></section></main>`;}
function playerCard(p:Player|undefined,i:number){
  if(!p)return `<div class="player-card empty"><span class="avatar">＋</span><strong>친구를 기다려요</strong></div>`;
  const mine=p.id===session?.playerId;const turn=room?.phase==='playing'&&p.id===room.turnPlayerId;
  const status=p.left?'나감':!p.connected?'연결 복구 중':room?.phase==='lobby'?(p.ready?'준비 완료':'준비 중'):!p.alive?'이번 라운드 실패':turn?'지금 차례':'대기 중';
  return `<div data-player="${p.id}" data-ready="${p.ready}" class="player-card color-${i} ${p.ready&&room?.phase==='lobby'?'ready':''} ${turn?'current':''} ${p.left?'out':''}"><div class="player-top">${p.id===room?.hostId?'<span class="host-tag">방장</span>':''}</div><span class="avatar">${escape(p.name.slice(0,1))}</span><strong>${escape(p.name)} ${mine?'<em>나</em>':''}</strong><span class="player-status">${p.ready&&room?.phase==='lobby'?'✓ ':''}${status}</span>${room?.hostId===session?.playerId&&!mine&&!p.left?`<button class="kick-button" data-action="kick" data-player-id="${p.id}" aria-label="${escape(p.name)} 강퇴" ${!connected?'disabled':''}>강퇴</button>`:''}${room?.phase!=='lobby'?`<span class="score">${p.score}<small> 점</small>${p.roundPenalty?`<small class="penalty-note">이번 라운드 −${p.roundPenalty}점</small>`:''}</span>`:''}</div>`;
}
function lobby(){const r=room!;const me=r.players.find(p=>p.id===session?.playerId);const host=r.hostId===session?.playerId;
 return `<main class="room-page"><div class="room-toolbar"><button class="text-button" data-action="leave">← 나가기</button><span class="room-code">ROOM <b>${r.code}</b><button aria-label="초대 코드 복사" data-action="copy-code">⧉</button></span></div><div class="lobby-layout"><section class="lobby-main"><h1>대기방</h1><div class="section-title"><h3>플레이어 <span>${r.players.length} / 4</span></h3></div><div class="players lobby-players">${Array.from({length:4},(_,i)=>playerCard(r.players[i],i)).join('')}</div><div class="round-settings"><label for="round-count">라운드 수</label><select id="round-count" ${!host||!connected?'disabled':''}>${Array.from({length:10},(_,i)=>`<option value="${i+1}" ${r.totalRounds===i+1?'selected':''}>${i+1} 라운드</option>`).join('')}</select><button class="outline" data-action="sound">알림 소리 ${soundEnabled?'켜짐':'꺼짐'}</button></div><details class="scoring-rules"><summary>점수 규칙</summary><p>정답 100점 · 추가 글자당 20점 · 빠른 입력 최대 200점</p><p>라운드 통과 +300점 · 실패 시 해당 라운드 점수 20% 차감(최대 150점)</p></details><div class="lobby-actions"><button class="${me?.ready?'ready-button':'dark'}" data-action="ready" ${!connected?'disabled':''}>${me?.ready?'✓ 준비 완료 · 취소하기':'준비하기'}</button>${host?`<button class="primary" data-action="start" ${!r.canStart||!connected?'disabled':''}>게임 시작 <span>→</span></button>`:`<span class="waiting-label">방장이 시작하기를 기다리고 있어요.</span>`}</div></section><aside class="invite-card"><h2>QR로 입장</h2><div class="qr-frame"><img id="qr" width="200" height="200" alt="이 대기방에 입장하는 QR 코드" ${qrCache?`src="${qrCache}"`:''}></div><div class="invite-code">${r.code}</div><button class="outline" data-action="copy-link">초대 링크 복사 <span>⧉</span></button><input class="invite-url" readonly aria-label="초대 주소" value="${escape(inviteUrl())}"></aside></div></main>`;
}
function wordMarkup(w:Word){const chars=Array.from(w.label);return chars.map((char,i)=>i===chars.length-1?`<mark class="word-glyph">${escape(char)}</mark>`:`<span class="word-glyph">${escape(char)}</span>`).join('');}
function meanings(w:Word){return `<details class="dictionary-panel"><summary><span class="dictionary-icon">가</span><span><strong>단어 뜻</strong></span><span class="expand-icon">＋</span></summary><div class="meaning-list">${w.meanings.map(m=>`<article><span class="category">${escape(categories[m.category]||m.category)}</span><p>${escape(m.definition)}</p><a href="${escape(m.source.url)}" target="_blank" rel="noopener noreferrer">${escape(m.source.name)} ↗</a></article>`).join('')}</div></details>`;}
function playing(){const r=room!;const me=r.players.find(p=>p.id===session?.playerId);const mine=r.turnPlayerId===me?.id;const current=r.players.find(p=>p.id===r.turnPlayerId);const w=r.currentWord!;
return `<main class="game-page"><div class="room-toolbar"><button class="text-button" data-action="leave">← 나가기</button><span class="room-code">ROOM <b>${r.code}</b></span></div><div class="players game-players" style="--player-count:${r.players.length}">${r.players.map(playerCard).join('')}</div><section class="arena ${mine?'my-turn':''}"><div class="turn-banner" role="status" aria-live="assertive">${mine?'내 차례':`${escape(current?.name)} 님의 차례`}<span>${r.round} / ${r.totalRounds} 라운드</span></div><div class="timer"><span class="timer-label" id="timer-label">남은 시간</span><strong id="time-value" role="timer">${(r.turnDurationMs/1000).toFixed(1)}</strong><span class="seconds">SEC</span></div><div class="time-track"><div id="time-progress"></div></div><div class="current-word" data-testid="current-word">${wordMarkup(w)}</div><div class="next-letter-panel"><span>다음 시작 글자</span><div class="word-prompt"><b>${escape(w.nextStarts.join(' / '))}</b></div></div>${wordError?`<div id="word-error" class="word-error" role="alert" aria-live="assertive" aria-atomic="true"><span class="warning-icon" aria-hidden="true">!</span><div><strong>정답이 아닙니다</strong><p>${escape(wordError)}</p></div></div>`:''}<form id="word-form" class="word-form ${wordError?'has-error':''}"><label class="sr-only" for="word-input">끝말잇기 단어</label><input enterkeyhint="send" type="text" id="word-input" value="${escape(wordDraft)}" aria-invalid="${Boolean(wordError)}" ${wordError?'aria-describedby="word-error"':''} maxlength="256" autocomplete="off" autocapitalize="off" spellcheck="false" placeholder="${mine?'단어 입력':'다른 참가자의 차례'}" ${!mine||!me?.alive||!connected?'disabled':''}><button class="primary" type="submit" id="submit-word" ${!mine||!connected?'disabled':''}>입력 <kbd>↵</kbd></button></form><p class="word-feedback" role="status">${escape(r.notice==='3초 후 시작합니다.'?'':r.notice||'')}</p></section><div class="game-bottom"><section class="history-panel"><div class="section-title"><h3>이어온 단어</h3><span>${r.acceptedCount}개 성공</span></div><div class="word-history">${r.history.map((h,i)=>`<div class="history-item ${i===r.history.length-1?'latest':''}"><small>${escape(h.name)}</small><strong>${escape(h.word.label)}</strong>${h.points?`<span class="earned-points">+${h.points}점</span>`:''}</div>`).join('<span class="history-arrow">→</span>')}</div></section>${meanings(w)}</div></main>`;}
function scoreTable(){const r=room!;return `<div class="result-players">${[...r.players].sort((a,b)=>Number(a.left)-Number(b.left)||b.score-a.score).map((p,i)=>`<div class="${p.left?'departed':''}"><span>${!p.left&&r.winnerIds.includes(p.id)?'★':i+1}</span><strong>${escape(p.name)}${p.id===session?.playerId?' (나)':''}${p.left?' · 퇴장':''}</strong><b>${p.score}<small> 점</small></b></div>`).join('')}</div>`;}
function finished(){const r=room!,between=r.phase==='intermission',last=r.roundResults.at(-1),failed=r.players.find(p=>p.id===last?.failedId);const names=r.winnerIds.map(id=>r.players.find(p=>p.id===id)?.name||'').map(escape).join(' · ');
return `<main class="results-page"><div class="room-toolbar"><button class="text-button" data-action="leave">← 나가기</button><span class="room-code">ROOM <b>${r.code}</b></span></div><section class="result-card"><span class="eyebrow">${between?`${r.round} / ${r.totalRounds} 라운드 결과`:'최종 결과'}</span><h1>${between?'라운드 종료':(names?`${names}<br>${r.winnerIds.length>1?'공동 우승':'최종 우승'}`:'게임 종료')}</h1>${between?`<p class="round-countdown" role="status">다음 라운드까지 <b id="round-wait">4</b>초</p><p>${escape(failed?.name||'참가자')} 님 ${failed?.left?'퇴장':'시간 초과'} · 나머지 참가자 +300점</p>`:`<p>${r.round} / ${r.totalRounds} 라운드 진행${r.round<r.totalRounds?' · 참가자 부족으로 조기 종료':''}</p>`}<h3 class="score-heading">누적 점수</h3>${scoreTable()}<details class="round-breakdown"><summary>라운드별 점수</summary>${r.roundResults.map(result=>`<div><h3>${result.round} 라운드</h3>${result.scores.map(s=>`<p>${escape(r.players.find(p=>p.id===s.playerId)?.name||'퇴장한 참가자')} <b>+${s.points}점</b>${s.penalty?` · 실패 감점 −${s.penalty}점 반영`:''}${result.winnerIds.includes(s.playerId)?' · 라운드 보너스':result.failedId===s.playerId?' · 라운드 실패':''}</p>`).join('')}</div>`).join('')}</details>${!between&&r.hostId===session?.playerId?'<button class="primary large" data-action="rematch">다시 준비하기 <span>↗</span></button>':''}</section></main>`;
}
function render(){
  if(composing){deferredRender=true;return;}
  const focused=document.activeElement as HTMLInputElement|null;
  const focusId=focused?.id;const value=focused?.value;const start=focused?.selectionStart;
  const detailsOpen=document.querySelector<HTMLDetailsElement>('.dictionary-panel')?.open;
  const oldView=app.dataset.view;const view=room?`${room.code}:${room.phase}:${room.round}`:'home';
  const oldPlayers=new Map(Array.from(app.querySelectorAll<HTMLElement>('[data-player]')).map(el=>[el.dataset.player,el.dataset.ready]));
  const oldTurn=app.dataset.turn;app.dataset.turn=String(room?.turnId??'');
  app.innerHTML=header()+(room?(room.phase==='lobby'?lobby():room.phase==='playing'?playing():finished()):home())+footer();
  if(detailsOpen){const details=document.querySelector<HTMLDetailsElement>('.dictionary-panel');if(details)details.open=true;}
  if(focusId){const next=document.getElementById(focusId) as HTMLInputElement|null;
    if(next&&!next.disabled&&(focusId!=='word-input'||oldTurn===app.dataset.turn)){if(value!==undefined)next.value=value;next.focus({preventScroll:true});if(start!==null&&start!==undefined)next.setSelectionRange(start,start);}}
  if(room?.phase==='playing'&&oldTurn!==app.dataset.turn&&room.turnPlayerId===session?.playerId){document.querySelector<HTMLInputElement>('#word-input')?.focus({preventScroll:true});}
  void updateQr().catch(()=>notify('QR 코드를 만들지 못했어요. 초대 링크로 입장해 주세요.'));
  paintTimer();
  if(oldView!==view)enterView(app);
  else {
    if(oldTurn!==app.dataset.turn)turnMotion(app);
    app.querySelectorAll<HTMLElement>('[data-player]').forEach(el=>{if(oldPlayers.get(el.dataset.player)!==el.dataset.ready)changedPlayer(el);});
  }
  app.dataset.view=view;
  if(pendingMotion&&room?.phase==='playing'){feedbackMotion(app,pendingMotion,motionPoints);pendingMotion=null;}
}
app.addEventListener('toggle',event=>{if(event.target instanceof HTMLDetailsElement)disclosure(event.target);},true);
app.addEventListener('input',event=>{const el=event.target as HTMLInputElement;if(el.id==='word-input')wordDraft=el.value;if(el.id==='nickname'){nick=el.value;sessionStorage.setItem('wordrelay-name',nick);}if(el.id==='room-code'){joinCode=el.value.toUpperCase().replace(/[^A-Z0-9]/g,'');el.value=joinCode;}});
app.addEventListener('change',event=>{const el=event.target as HTMLSelectElement;if(el.id==='round-count')send({type:'configure',rounds:Number(el.value)});});
app.addEventListener('compositionstart',()=>{composing=true;});
app.addEventListener('compositionend',()=>{
 composing=false;
 const turn=room?.turnId, queued=submitAfterComposition;submitAfterComposition=false;
 // Wait for the committed Hangul input event before reading the final syllable.
 window.setTimeout(()=>{if(deferredRender){deferredRender=false;render();}if(queued&&turn===room?.turnId)submitWord();},0);
});
function submitWord(){
 if(composing){submitAfterComposition=true;return;}
 if(!room||pendingTurn!==null||!connected||room.turnPlayerId!==session?.playerId||serverNow()<(room.startsAt||0)||serverNow()>=(room.deadline||0))return;
 const input=document.querySelector<HTMLInputElement>('#word-input');const word=input?.value.trim();if(!word)return;
 pendingTurn=room.turnId;wordError='';send({type:'submit',word,turnId:room.turnId});paintTimer();
}
app.addEventListener('keydown',event=>{
 if((event.target as HTMLElement).id!=='word-input'||event.key!=='Enter'||event.repeat)return;
 if(event.isComposing||composing){submitAfterComposition=true;return;}
 event.preventDefault();submitWord();
});
app.addEventListener('beforeinput',event=>{
 const e=event as InputEvent;
 if((e.target as HTMLElement).id==='word-input'&&['insertLineBreak','insertParagraph'].includes(e.inputType)){e.preventDefault();submitWord();}
});
app.addEventListener('submit',event=>{
 event.preventDefault();const form=event.target as HTMLFormElement;
 if(form.id==='entry-form'){
  nick=(document.querySelector<HTMLInputElement>('#nickname')?.value||'').trim();if(!nick||Array.from(nick).length>12){notify('이름을 1~12글자로 입력해 주세요.');return;}
  const action=(event as SubmitEvent).submitter?.getAttribute('value')||'create';
  if(action==='join'&&!/^[A-Z0-9]{6}$/.test(joinCode)){notify('6자리 초대 코드를 입력해 주세요.');return;}
  sessionStorage.setItem('wordrelay-name',nick);connect(action==='join'?{type:'join',code:joinCode,name:nick}:{type:'create',name:nick});
 }else if(form.id==='word-form'){
  submitWord();
 }
});
app.addEventListener('click',async event=>{
 const target=(event.target as HTMLElement).closest<HTMLElement>('[data-action]');if(!target)return;
 const action=target.dataset.action;
 if(action==='ready'){const me=room?.players.find(p=>p.id===session?.playerId);send({type:'ready',ready:!me?.ready});}
 if(action==='kick'){const p=room?.players.find(p=>p.id===target.dataset.playerId);if(p&&window.confirm(`${p.name} 님을 강퇴할까요?`))send({type:'kick',playerId:p.id});}
 if(action==='sound'){soundEnabled=!soundEnabled;if(soundEnabled){audioContext??=new AudioContext();void audioContext.resume();}render();}
 if(action==='start')send({type:'start'});
 if(action==='rematch')send({type:'rematch'});
 if(action==='leave'){if(room?.phase==='playing'&&!window.confirm('나가면 이번 라운드가 종료됩니다. 나갈까요?'))return;if(connected)send({type:'leave'});else returnHome();}
 if(action==='copy-code'||action==='copy-link'){
   try{await navigator.clipboard.writeText(action==='copy-code'?room!.code:inviteUrl());notify(action==='copy-code'?'초대 코드를 복사했어요.':'초대 링크를 복사했어요.');}
   catch{document.querySelector<HTMLInputElement>('.invite-url')?.select();notify('아래 초대 주소를 길게 눌러 복사해 주세요.');}
 }
});
let enabledBefore=false;
function paintTimer(){
 if(room?.phase==='intermission'){const el=document.querySelector('#round-wait');if(el)el.textContent=String(Math.max(0,Math.ceil(((room.nextRoundAt||0)-serverNow())/1000)));return;}
 if(room?.phase!=='playing')return;
 turnAlert();
 const now=serverNow(),countdown=Math.max(0,(room.startsAt||0)-now),left=Math.max(0,(room.deadline||0)-now);
 const shown=Math.min(room.turnDurationMs,displayTime(left)), stage=countdown>0?'normal':timerStage(shown);
 const time=document.querySelector('#time-value');if(time)time.textContent=countdown>0?String(Math.ceil(countdown/1000)):(Math.ceil(shown/(shown<=1000?10:100))/(shown<=1000?100:10)).toFixed(shown<=1000?2:1);
 const label=document.querySelector('#timer-label');if(label)label.textContent=countdown>0?'시작까지':'남은 시간';
 const progress=document.querySelector<HTMLElement>('#time-progress');if(progress)progress.style.transform=`scaleX(${countdown>0?1:shown/room.turnDurationMs})`;
 const arena=document.querySelector<HTMLElement>('.arena');if(arena)arena.dataset.timer=stage;
 document.querySelector('.timer')?.classList.toggle('urgent',stage==='critical');
 const enabled=connected&&room.turnPlayerId===session?.playerId&&countdown===0&&left>0&&pendingTurn===null;
 const input=document.querySelector<HTMLInputElement>('#word-input');const button=document.querySelector<HTMLButtonElement>('#submit-word');
 if(input)input.disabled=!enabled;if(button)button.disabled=!enabled;
 if(enabled&&!enabledBefore)input?.focus({preventScroll:true});enabledBefore=enabled;
}
function animate(){paintTimer();requestAnimationFrame(animate);}requestAnimationFrame(animate);
window.addEventListener('offline',()=>{if(session)ws?.close();});
window.addEventListener('online',()=>{if(!connected&&!connecting&&session){clearTimeout(reconnectTimer);connect({type:'join',code:session.code,token:session.token,name:nick});}});
render();if(session)connect({type:'join',code:session.code,token:session.token,name:nick});
