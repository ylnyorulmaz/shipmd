/**
 * FLIP helper: measure cards, change the DOM, then animate each card from where it was
 * to where it is now. Used for siblings making room during a drag and for remote moves.
 */

export const prefersReducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

const EASE = 'cubic-bezier(.2,.8,.2,1)';

export function flip(scope: ParentNode, mutate: () => void, opts: { exclude?: string; duration?: number } = {}) {
  if (prefersReducedMotion()) return mutate();

  const first = new Map<HTMLElement, DOMRect>();
  for (const el of scope.querySelectorAll<HTMLElement>('[data-card-id]')) {
    if (el.dataset.cardId === opts.exclude || el.offsetParent === null) continue;
    // Visual position, including any FLIP still running, so interrupted moves continue smoothly.
    first.set(el, el.getBoundingClientRect());
  }

  mutate(); // Solid updates the DOM synchronously.

  for (const [el, before] of first) {
    if (!el.isConnected || el.offsetParent === null) continue;
    for (const a of el.getAnimations()) if (a.id === 'flip') a.cancel();
    const after = el.getBoundingClientRect();
    const dx = before.left - after.left;
    const dy = before.top - after.top;
    if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5) continue;
    const anim = el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], {
      duration: opts.duration ?? 200,
      easing: EASE,
    });
    anim.id = 'flip';
  }
}
