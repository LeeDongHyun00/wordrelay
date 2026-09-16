import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
const url=process.env.WS_URL||'ws://127.0.0.1:3000/ws';
const dict=JSON.parse(readFileSync(new URL('../data/dictionary.json',import.meta.url),'utf8'));
for(const file of dict.supplements||[])dict.entries=dict.entries.concat(JSON.parse(gunzipSync(readFileSync(new URL('../data/'+file,import.meta.url))).toString()).entries);
const byFirst=new Map();for(const e of dict.entries){if(!byFirst.has(e.firstSyllable))byFirst.set(e.firstSyllable,[]);byFirst.get(e.firstSyllable).push(e);}
const clients=[];
class Peer {
 constructor(){this.ws=new WebSocket(url);this.heartbeat=setInterval(()=>{if(this.ws.readyState===WebSocket.OPEN)this.send({type:'ping',sentAt:Date.now()});},1000);this.ws.addEventListener('close',()=>clearInterval(this.heartbeat));this.events=[];this.pending=[];clients.push(this);this.ws.addEventListener('message',e=>{const m=JSON.parse(e.data);this.events.push(m);for(const wake of this.pending.splice(0))wake();});}
 async open(){await new Promise((resolve,reject)=>{this.ws.addEventListener('open',resolve,{once:true});this.ws.addEventListener('error',reject,{once:true});});}
 send(data){this.ws.send(JSON.stringify(data));}
 async wait(predicate,after=0,timeout=10000){const deadline=Date.now()+timeout;while(Date.now()<deadline){const found=this.events.slice(after).find(predicate);if(found)return found;await new Promise(resolve=>{const t=setTimeout(resolve,50);this.pending.push(()=>{clearTimeout(t);resolve();});});}throw new Error('Timed out: '+JSON.stringify(this.events.slice(-3)));}
 async act(data,predicate){const after=this.events.length;this.send(data);return this.wait(predicate,after);}
 close(){clearInterval(this.heartbeat);this.ws.close();}
}
async function connect(message){const p=new Peer();await p.open();p.send(message);return p;}
const sleep=ms=>new Promise(r=>setTimeout(r,Math.max(0,ms)));
test('real WebSockets: four players, ready, countdown, 55 answers, floor, resume, round end, rematch', {timeout:40000},async()=>{
 try{
  const a=await connect({type:'create',name:'방장'});const welcome=await a.wait(m=>m.type==='welcome');const code=welcome.code;
  const peers=[a];const ids=[welcome.playerId];const welcomes=[welcome];
  for(const name of ['둘째','셋째','넷째']){const p=await connect({type:'join',code,name});const w=await p.wait(m=>m.type==='welcome');peers.push(p);ids.push(w.playerId);welcomes.push(w);}
  const full=await connect({type:'join',code,name:'다섯째'});assert.equal((await full.wait(m=>m.type==='error')).code,'ROOM_FULL');
  const bad=await a.act({type:'start'},m=>m.type==='error');assert.equal(bad.code,'NOT_READY');
  const hostOnly=await peers[1].act({type:'start'},m=>m.type==='error');assert.equal(hostOnly.code,'HOST_ONLY');
  await a.act({type:'configure',rounds:1},m=>m.type==='state'&&m.room.totalRounds===1);
  for(let i=0;i<4;i++)await peers[i].act({type:'ready',ready:true},m=>m.type==='state'&&m.room.players.find(p=>p.id===ids[i])?.ready);
  let room=(await a.act({type:'start'},m=>m.type==='state'&&m.room.phase==='playing')).room;
  assert.equal(room.turnDurationMs,6000);assert.equal(room.players.length,4);
  const late=await connect({type:'join',code,name:'늦은입장'});assert.equal((await late.wait(m=>m.type==='error')).code,'ALREADY_STARTED');
  const current=peers[ids.indexOf(room.turnPlayerId)];
  const early=await current.act({type:'submit',word:'사과',turnId:room.turnId},m=>m.type==='error');assert.equal(early.code,'COUNTDOWN');
  await sleep(room.startsAt-Date.now()+30);
  const stale=await current.act({type:'submit',word:'사과',turnId:0},m=>m.type==='error');assert.equal(stale.code,'STALE_TURN');
  const used=new Set(room.history.map(h=>h.word.reading));
  for(let i=1;i<=55;i++){
   for(const h of room.history)used.add(h.word.reading);
   const candidates=room.currentWord.nextStarts.flatMap(s=>byFirst.get(s)||[]).filter(e=>!used.has(e.reading));
   const word=candidates.sort((a,b)=>(byFirst.get(b.lastSyllable)?.length||0)-(byFirst.get(a.lastSyllable)?.length||0))[0];assert(word);
   const p=peers[ids.indexOf(room.turnPlayerId)];
   const response=await p.act({type:'submit',word:word.label,turnId:room.turnId},m=>m.type==='state'&&m.room.acceptedCount===i);
   room=response.room;used.add(word.reading);assert.equal(room.turnDurationMs,Math.max(1000,6000-i*100));
  }
  // Replace an existing connection with its secret; it keeps the same slot, turn and deadline.
  const old=room.players.find(p=>p.id===room.turnPlayerId);const oldIndex=ids.indexOf(old.id);const oldDeadline=room.deadline;
  const reconnect=await connect({type:'join',code,name:'이름변경시도',token:welcomes[oldIndex].token});
  const resumed=await reconnect.wait(m=>m.type==='welcome');assert.equal(resumed.playerId,old.id);
  const restored=(await reconnect.wait(m=>m.type==='state')).room;
  assert.equal(restored.players.length,4);assert.equal(restored.players.find(p=>p.id===old.id).name,old.name);
  assert(Math.abs(restored.deadline-oldDeadline)<20);assert.equal(restored.acceptedCount,55);
  const finished=(await reconnect.wait(m=>m.type==='state'&&m.room.phase==='finished',0,8000)).room;
  assert.equal(finished.players.filter(p=>p.alive&&!p.left).length,3);assert(finished.winnerIds.length);assert.equal(finished.roundResults[0].winnerIds.length,3);
  assert.equal(finished.turnDurationMs,1000);
  const hostId=finished.hostId;const host=hostId===resumed.playerId?reconnect:peers[ids.indexOf(hostId)];
  const reset=(await host.act({type:'rematch'},m=>m.type==='state'&&m.room.phase==='lobby')).room;
  assert.equal(reset.acceptedCount,0);assert.equal(reset.currentWord,null);assert(reset.players.every(p=>!p.ready));
  console.log('Verified 55 consecutive accepted answers: 6000ms → 1000ms; four synchronized clients.');
 }finally{for(const p of clients)p.close();}
});

test('host kick removes a guest, rejects forged authority and invalidates resume', {timeout:15000},async()=>{
 const peers=[];
 try {
  const host=await connect({type:'create',name:'방장'});peers.push(host);const h=await host.wait(m=>m.type==='welcome');
  const guest=await connect({type:'join',code:h.code,name:'참가자'});peers.push(guest);const g=await guest.wait(m=>m.type==='welcome');
  assert.equal((await guest.act({type:'kick',playerId:h.playerId},m=>m.type==='error')).code,'HOST_ONLY');
  assert.equal((await host.act({type:'kick',playerId:h.playerId},m=>m.type==='error')).code,'CANNOT_KICK_SELF');
  const result=await host.act({type:'kick',playerId:g.playerId},m=>m.type==='state'&&m.room.players.length===1);
  assert.equal(result.room.canStart,false);await guest.wait(m=>m.type==='kicked');
  const resume=await connect({type:'join',code:h.code,name:'복귀',token:g.token});peers.push(resume);
  assert.equal((await resume.wait(m=>m.type==='error')).code,'SESSION_EXPIRED');
 } finally {for(const p of peers)p.close();}
});


test('two rounds advance automatically; totals persist; disconnected seat expires after six seconds', {timeout:60000},async()=>{
 const peers=[];
 try{
  const host=await connect({type:'create',name:'방장'});peers.push(host);const h=await host.wait(m=>m.type==='welcome');
  const guest=await connect({type:'join',code:h.code,name:'손님'});peers.push(guest);const g=await guest.wait(m=>m.type==='welcome');
  assert.equal((await guest.act({type:'configure',rounds:2},m=>m.type==='error')).code,'HOST_ONLY');
  await host.act({type:'configure',rounds:2},m=>m.type==='state'&&m.room.totalRounds===2);
  for(const p of peers)await p.act({type:'ready',ready:true},m=>m.type==='state');
  await host.act({type:'start'},m=>m.type==='state'&&m.room.phase==='playing');
  const between=(await host.wait(m=>m.type==='state'&&m.room.phase==='intermission',0,16000)).room;
  assert.equal(between.round,1);assert.equal(between.roundResults.length,1);assert.equal(between.players.reduce((sum,p)=>sum+p.score,0),300);
  const second=(await host.wait(m=>m.type==='state'&&m.room.round===2&&m.room.phase==='playing',0,6000)).room;
  assert.equal(second.turnDurationMs,6000);assert.equal(second.acceptedCount,0);assert(second.players.every(p=>p.alive));assert.equal(second.players.reduce((sum,p)=>sum+p.score,0),300);
  const final=(await host.wait(m=>m.type==='state'&&m.room.phase==='finished',0,16000)).room;
  assert.equal(final.roundResults.length,2);assert.equal(final.players.reduce((sum,p)=>sum+p.score,0),600);
  const best=Math.max(...final.players.map(p=>p.score));assert.deepEqual(new Set(final.winnerIds),new Set(final.players.filter(p=>p.score===best).map(p=>p.id)));
  await host.act({type:'rematch'},m=>m.type==='state'&&m.room.phase==='lobby');
  const after=host.events.length;guest.close();
  await host.wait(m=>m.type==='state'&&m.room.players.length===1,after,8500);
  const resume=await connect({type:'join',code:h.code,name:'복귀',token:g.token});peers.push(resume);
  assert.equal((await resume.wait(m=>m.type==='error')).code,'SESSION_EXPIRED');
 }finally{for(const p of peers)p.close();}
});


test('server expires a silent connection without waiting for TCP close', {timeout:12000},async()=>{
 const host=await connect({type:'create',name:'호스트'});const h=await host.wait(m=>m.type==='welcome');
 const guest=await connect({type:'join',code:h.code,name:'신호없음'});const g=await guest.wait(m=>m.type==='welcome');
 try {
 // Welcome is sent on the guest socket; the host can still receive its older
 // one-player snapshot afterward. Observe membership before testing removal.
 await host.wait(m=>m.type==='state'&&m.room.players.some(p=>p.id===g.playerId));
 clearInterval(guest.heartbeat);
 // Establish a fresh server lease before measuring the silent interval.
 await guest.act({type:'ping',sentAt:Date.now()},m=>m.type==='pong');
 const after=host.events.length;const start=performance.now();
 const state=(await host.wait(m=>m.type==='state'&&!m.room.players.some(p=>p.id===g.playerId),after,8500)).room;
 assert(performance.now()-start>=5000);assert.equal(state.players.length,1);
 }finally{host.close();guest.close();}
});

test('expanded dictionary accepts 비비 and 비수 with sourced explanations', {timeout:30000},async()=>{
 const peers=[];
 try {
  const host=await connect({type:'create',name:'사전검증'});peers.push(host);
  const h=await host.wait(m=>m.type==='welcome');
  const guest=await connect({type:'join',code:h.code,name:'사전검증2'});peers.push(guest);
  const g=await guest.wait(m=>m.type==='welcome');const ids=[h.playerId,g.playerId];
  await host.act({type:'configure',rounds:1},m=>m.type==='state'&&m.room.totalRounds===1);
  for(const p of peers)await p.act({type:'ready',ready:true},m=>m.type==='state'&&m.room.players.some(x=>x.ready));
  let room=(await host.act({type:'start'},m=>m.type==='state'&&m.room.phase==='playing')).room;
  await sleep(room.startsAt-Date.now()+60);
  const queue=room.currentWord.nextStarts.map(s=>({s,path:[]}));const seen=new Set(room.currentWord.nextStarts);let path;
  for(let cursor=0;cursor<queue.length;cursor++){const node=queue[cursor];if(node.s==='비'){path=node.path;break;}
   for(const e of byFirst.get(node.s)||[])if(e.reading!==room.currentWord.reading&&!seen.has(e.lastSyllable)){seen.add(e.lastSyllable);queue.push({s:e.lastSyllable,path:[...node.path,e.label]});}
  }
  assert(path,'reachable chain to 비');
  for(const word of [...path,'비비','비수']){
   const p=peers[ids.indexOf(room.turnPlayerId)];const count=room.acceptedCount;
   room=(await p.act({type:'submit',word,turnId:room.turnId},m=>m.type==='state'&&m.room.acceptedCount===count+1)).room;
   if(['비비','비수'].includes(word)){
    const accepted=room.currentWord;assert.equal(accepted.label,word);
    assert(accepted.meanings.some(m=>m.definition&&m.source.url.startsWith('https://opendict.korean.go.kr/')));
    assert(!accepted.meanings[0].definition.includes('규범 표기는'));
    if(word==='비수')assert(accepted.meanings[0].definition.includes('칼'));
    console.log(word,accepted.meanings[0].definition,room.dictionaryVersion);
   }
  }
 }finally{for(const p of peers){if(p.ws.readyState===WebSocket.OPEN)p.send({type:'leave'});p.close();}}
});


test('four players: first timeout ends round and everyone returns for round two', {timeout:26000},async()=>{
 const peers=[];
 try {
  const host=await connect({type:'create',name:'라운드방장'});peers.push(host);const h=await host.wait(m=>m.type==='welcome');
  for(let i=1;i<4;i++){const p=await connect({type:'join',code:h.code,name:'참가자'+i});peers.push(p);await p.wait(m=>m.type==='welcome');}
  await host.act({type:'configure',rounds:2},m=>m.type==='state'&&m.room.totalRounds===2);
  for(const p of peers)await p.act({type:'ready',ready:true},m=>m.type==='state');
  const first=(await host.act({type:'start'},m=>m.type==='state'&&m.room.phase==='playing')).room;
  assert.equal(first.deadline-first.startsAt,10000);
  const result=(await host.wait(m=>m.type==='state'&&m.room.phase==='intermission',0,16000)).room;
  assert.equal(result.roundResults[0].failedId,first.turnPlayerId);assert.equal(result.roundResults[0].winnerIds.length,3);
  assert.equal(result.players.reduce((sum,p)=>sum+p.score,0),900);
  const second=(await host.wait(m=>m.type==='state'&&m.room.phase==='playing'&&m.room.round===2,0,6000)).room;
  assert(second.players.every(p=>p.alive));assert.equal(second.roundResults.length,1);
 }finally{for(const p of peers){if(p.ws.readyState===WebSocket.OPEN)p.send({type:'leave'});p.close();}}
});
