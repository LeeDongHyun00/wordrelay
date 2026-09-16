const preference = matchMedia('(prefers-reduced-motion: reduce)');
const spring = 'cubic-bezier(.16,1,.3,1)';
function move(el: Element|null, frames: Keyframe[], duration=460, delay=0) {
  if (!el || preference.matches) return;
  el.animate(frames, {duration, delay, easing:spring});
}
export function enterView(root: HTMLElement) {
  move(root.querySelector('main'), [{opacity:0,transform:'translateY(12px)'},{opacity:1,transform:'translateY(0)'}],520);
  root.querySelectorAll('.player-card, .result-players > div').forEach((el,i)=>
    move(el,[{opacity:0,transform:'translateY(9px) scale(.98)'},{opacity:1,transform:'translateY(0) scale(1)'}],440,i*35));
}
export function turnMotion(root: HTMLElement) {
  move(root.querySelector('.turn-banner'),[{opacity:.5,transform:'translateY(-6px)'},{opacity:1,transform:'translateY(0)'}]);
  move(root.querySelector('.player-card.current'),[{transform:'scale(.975)'},{transform:'scale(1)'}],520);
}
export function feedbackMotion(root: HTMLElement, kind:'success'|'error', points=0) {
  if (preference.matches) return;
  if (kind==='error') {
    move(root.querySelector('.word-form'),[{transform:'translateX(0)'},{transform:'translateX(-7px)',offset:.2},{transform:'translateX(5px)',offset:.4},{transform:'translateX(-3px)',offset:.65},{transform:'translateX(0)'}],380);
    move(root.querySelector('.word-error'),[{opacity:0,transform:'translateY(-5px) scale(.98)'},{opacity:1,transform:'translateY(0) scale(1)'}],300);
    root.querySelector('.arena')?.classList.add('impact-error');
    return;
  }
  root.querySelector('.arena')?.classList.add('impact-success');
  root.querySelectorAll('.word-glyph').forEach((el,i)=>move(el,[
    {opacity:.2,transform:'translateY(12px) scale(.88)'},
    {opacity:1,transform:'translateY(-2px) scale(1.04)',offset:.6},
    {opacity:1,transform:'translateY(0) scale(1)'}],480,Math.min(i,20)*15));
  move(root.querySelector('.next-letter-panel .word-prompt b'),[{transform:'scale(.9)'},{transform:'scale(1.04)',offset:.55},{transform:'scale(1)'}],540);
  move(root.querySelector('.history-item.latest'),[{opacity:0,transform:'translateX(-12px)'},{opacity:1,transform:'translateX(0)'}]);
  const arena=root.querySelector('.arena');
  if(arena&&points){const label=document.createElement('span');label.className='impact-points';label.textContent=`+${points}`;label.setAttribute('aria-hidden','true');arena.append(label);
    const animation=label.animate([{opacity:0,transform:'translate(-50%,8px) scale(.8)'},{opacity:1,transform:'translate(-50%,-8px) scale(1)',offset:.25},{opacity:0,transform:'translate(-50%,-40px) scale(.95)'}],{duration:850,easing:'ease-out'});animation.onfinish=()=>label.remove();}
}
export function changedPlayer(el:Element|null) {
  move(el,[{transform:'scale(.97)',opacity:.7},{transform:'scale(1)',opacity:1}],380);
}
export function disclosure(el:HTMLDetailsElement) {
  if(el.open)move(el.querySelector('.meaning-list')||el.querySelector('p'),[{opacity:0,transform:'translateY(-5px)'},{opacity:1,transform:'translateY(0)'}],300);
}
