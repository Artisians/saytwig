'use strict';
// A scroll illustration, never live audio. No network, capture, perpetual loop,
// or per-scroll worker: updates coalesce into one pending animation frame.
const story = document.getElementById('story');
const stage = document.getElementById('stage');
const scenes = [...document.querySelectorAll('.scene')];
const controls = [...document.querySelectorAll('[data-scene]')];
const footer = document.getElementById('stage-footer');
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
const isReduced = () => reducedMotion.matches;
let frame = 0;
let currentScene = -1;
const clamp = (value) => Math.min(1, Math.max(0, value));
function update() {
  frame = 0;
  if (isReduced()) return;
  const bounds = story.getBoundingClientRect();
  const distance = Math.max(1, bounds.height - window.innerHeight);
  const progress = clamp(-bounds.top / distance);
  stage.style.setProperty('--progress', progress.toFixed(4));
  stage.style.setProperty('--voice-opacity', clamp(1 - progress * 4).toFixed(4));
  stage.style.setProperty('--words-opacity', clamp(Math.min((progress - .2) * 5, (.8 - progress) * 5)).toFixed(4));
  stage.style.setProperty('--local-opacity', clamp((progress - .65) / .25).toFixed(4));
  stage.style.setProperty('--word-reveal', clamp((progress - .2) / .28).toFixed(4));
  const active = progress < .28 ? 0 : progress < .73 ? 1 : 2;
  if (active !== currentScene) {
    currentScene = active;
    scenes.forEach((scene, index) => {
      scene.classList.toggle('is-active', index === active);
      scene.inert = index !== active;
      scene.setAttribute('aria-hidden', String(index !== active));
    });
    controls.forEach((button, index) => button.setAttribute('aria-pressed', String(index === active)));
  }
}
function schedule() {
  if (!frame && !isReduced()) frame = window.requestAnimationFrame(update);
}
function goToScene(index) {
  if (isReduced()) {
    scenes[index].scrollIntoView({ behavior: 'auto', block: 'start' });
    return;
  }
  const bounds = story.getBoundingClientRect();
  const progress = [0, .5, .96][index];
  window.scrollTo({ top: window.scrollY + bounds.top + progress * Math.max(1, bounds.height - window.innerHeight), behavior: 'smooth' });
}
function syncMotion() {
  if (frame) window.cancelAnimationFrame(frame);
  frame = 0;
  currentScene = -1;
  document.documentElement.classList.toggle('motion-paused', isReduced());
  document.body.classList.toggle('motion-ready', !isReduced());
  footer.hidden = isReduced();
  scenes.forEach((scene) => {
    scene.inert = false;
    scene.removeAttribute('aria-hidden');
  });
  if (!isReduced()) update();
}
controls.forEach((button, index) => button.addEventListener('click', () => goToScene(index)));
document.querySelectorAll('a[href="#voice"], a[href="#words"]').forEach((link) => {
  link.addEventListener('click', (event) => {
    event.preventDefault();
    history.replaceState(null, '', link.hash);
    goToScene(link.hash === '#words' ? 1 : 0);
  });
});
window.addEventListener('scroll', schedule, { passive: true });
window.addEventListener('resize', schedule, { passive: true });
reducedMotion.addEventListener('change', syncMotion);
document.addEventListener('visibilitychange', () => {
  if (document.hidden && frame) {
    window.cancelAnimationFrame(frame);
    frame = 0;
  } else if (!document.hidden) schedule();
});
syncMotion();
if (!isReduced() && window.scrollY < 10) {
  stage.classList.add('intro');
  const lastEntranceBar = document.querySelector('.wave span:nth-child(31)');
  function finishEntrance(event) {
    if (event.target !== lastEntranceBar) return;
    stage.classList.remove('intro');
    stage.removeEventListener('animationend', finishEntrance);
  }
  stage.addEventListener('animationend', finishEntrance);
}
