import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const url=process.env.WS_URL||'ws://127.0.0.1:3000/ws';
const dict=JSON.parse(readFileSync(new URL('../data/dictionary.json',import.meta.url),'utf8'));
const clients=[];
class Peer {
 constructor(){this.ws=new WebSocket(url);this.events=[];this.pending=[];clients.push(this);this.ws.addEventListener('message',e=>{const m=JSON.parse(e.data);this.events.push(m);for(const wake of this.pending.splice(0))wake();});}
 async open(){await new Promise((resolve,reject)=>{this.ws.addEventListener('open',resolve,{once:true});this.ws.addEventListener('error',reject,{once:true});});}
 send(data){this.ws.send(JSON.stringify(data));}
 async wait(predicate,after=0,timeout=10000){const deadline=Date.now()+timeout;while(Date.now()<deadline){const found=this.events.slice(after).find(predicate);if(found)return found;await new Promise(resolve=>{const t=setTimeout(resolve,50);this.pending.push(()=>{clearTimeout(t);resolve();});});}throw new Error('Timed out: '+JSON.stringify(this.events.slice(-3)));}
 async act(data,predicate){const after=this.events.length;this.send(data);return this.wait(predicate,after);}
 close(){this.ws.close();}
}
async function connect(message){const p=new Peer();await p.open();p.send(message);return p;}
const sleep=ms=>new Promise(r=>setTimeout(r,Math.max(0,ms)));
test('real WebSockets: four players, ready, countdown, 55 answers, floor, resume, eliminate, rematch', {timeout:40000},async()=>{
 try{
  const a=await connect({type:'create',name:'방장'});const welcome=await a.wait(m=>m.type==='welcome');const code=welcome.code;
  const peers=[a];const ids=[welcome.playerId];const welcomes=[welcome];
  for(const name of ['둘째','셋째','넷째']){const p=await connect({type:'join',code,name});const w=await p.wait(m=>m.type==='welcome');peers.push(p);ids.push(w.playerId);welcomes.push(w);}
  const full=await connect({type:'join',code,name:'다섯째'});assert.equal((await full.wait(m=>m.type==='error')).code,'ROOM_FULL');
  const bad=await a.act({type:'start'},m=>m.type==='error');assert.equal(bad.code,'NOT_READY');
  const hostOnly=await peers[1].act({type:'start'},m=>m.type==='error');assert.equal(hostOnly.code,'HOST_ONLY');
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
   const word=dict.entries.find(e=>room.currentWord.nextStarts.includes(e.firstSyllable)&&!used.has(e.reading));assert(word);
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
  assert.equal(finished.players.filter(p=>p.alive&&!p.left).length,1);assert(finished.winnerId);
  assert.equal(finished.turnDurationMs,1000);
  const hostId=finished.hostId;const host=hostId===resumed.playerId?reconnect:peers[ids.indexOf(hostId)];
  const reset=(await host.act({type:'rematch'},m=>m.type==='state'&&m.room.phase==='lobby')).room;
  assert.equal(reset.acceptedCount,0);assert.equal(reset.currentWord,null);assert(reset.players.every(p=>!p.ready));
  console.log('Verified 55 consecutive accepted answers: 6000ms → 1000ms; four synchronized clients.');
 }finally{for(const p of clients)p.close();}
});
