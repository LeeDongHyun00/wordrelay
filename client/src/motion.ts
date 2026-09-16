const preference = matchMedia('(prefers-reduced-motion: reduce)');
const settle = 'cubic-bezier(.2,.8,.2,1)';
const active = new Set<Animation>();
function move(el: Element|null, frames: Keyframe[], duration=240, delay=0, easing=settle) {
  if (!el || preference.matches) return;
  const animation=el.animate(frames,{duration,delay,easing,fill:'backwards'});
  active.add(animation);
  const release=()=>active.delete(animation);
  animation.addEventListener('finish',release,{once:true});
  animation.addEventListener('cancel',release,{once:true});
  return animation;
}
// Rendering replaces elements. Cancel their animations rather than retaining
// detached targets or allowing feedback from an old turn to linger.
export function clearMotion() {
  for(const animation of active)animation.cancel();
  active.clear();
}
preference.addEventListener('change',()=>{if(preference.matches)clearMotion();});
export function enterView(root: HTMLElement) {
  move(root.querySelector('main'),[{opacity:.6,transform:'translateY(6px)'},{opacity:1,transform:'none'}],280);
  // Only results need a sequence; input and player controls appear together.
  root.querySelectorAll('.result-players > div').forEach((el,i)=>
    move(el,[{opacity:.5,transform:'translateY(5px)'},{opacity:1,transform:'none'}],240,i*25));
}
export function turnMotion(root: HTMLElement, quiet=false) {
  // During successful answers the word already moves; highlight the recipient
  // without also moving the banner or the whole board.
  if(!quiet)move(root.querySelector('.turn-banner'),[{opacity:.55},{opacity:1}],180);
  move(root.querySelector('.player-card.current'),[
    {boxShadow:'0 0 0 0 #9ac14d00'},
    {boxShadow:'0 0 0 3px #9ac14d55',offset:.3},
    {boxShadow:'0 0 0 0 #9ac14d00'}],360);
}
export function feedbackMotion(root: HTMLElement, kind:'success'|'error', points=0, playerId:string|null=null) {
  if(preference.matches)return;
  root.querySelector('.arena')?.classList.add(kind==='success'?'impact-success':'impact-error');
  if(kind==='error'){
    move(root.querySelector('#word-input'),[
      {transform:'translateX(0)'},{transform:'translateX(-4px)',offset:.2},
      {transform:'translateX(3px)',offset:.45},{transform:'translateX(-1px)',offset:.7},
      {transform:'translateX(0)'}],220,0,'ease-in-out');
    move(root.querySelector('.word-error'),[{opacity:0,transform:'translateY(-3px)'},{opacity:1,transform:'none'}],180);
    return;
  }
  const glyphs=root.querySelectorAll('.word-glyph');
  glyphs.forEach((el,i)=>move(el,[{opacity:.5,transform:'translateY(5px)'},{opacity:1,transform:'none'}],220,glyphs.length>1?i/(glyphs.length-1)*50:0));
  move(root.querySelector('.next-letter-panel .word-prompt b'),[{transform:'scale(.97)'},{transform:'scale(1)'}],240);
  move(root.querySelector('.history-item.latest'),[{opacity:.5,transform:'translateX(-5px)'},{opacity:1,transform:'none'}],200);
  const player=Array.from(root.querySelectorAll<HTMLElement>('[data-player]')).find(el=>el.dataset.player===playerId);
  if(player&&points){
    const label=document.createElement('span');label.className='impact-points';label.textContent=`+${points}`;label.setAttribute('aria-hidden','true');player.append(label);
    const animation=move(label,[{opacity:0,transform:'translate(-50%,4px)'},{opacity:1,transform:'translate(-50%,-2px)',offset:.18},{opacity:1,transform:'translate(-50%,-6px)',offset:.65},{opacity:0,transform:'translate(-50%,-12px)'}],600,0,'ease-out');
    const remove=()=>label.remove();animation?.addEventListener('finish',remove,{once:true});animation?.addEventListener('cancel',remove,{once:true});
    move(player.querySelector('.score'),[{color:'#8caf39'},{color:'#2d4617'}],320);
  }
}
export function changedPlayer(el:Element|null) {
  move(el,[{opacity:.65},{opacity:1}],180);
}
export function disclosure(el:HTMLDetailsElement) {
  if(el.open)move(el.querySelector('.meaning-list')||el.querySelector('p'),[{opacity:0,transform:'translateY(-3px)'},{opacity:1,transform:'none'}],180);
}
