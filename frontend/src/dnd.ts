import { batch, createSelector, createSignal } from 'solid-js';
import { flip, prefersReducedMotion } from './flip';
import { board, GAP, moveCard, sortedCardsIn } from './store';

/**
 * Pointer-based drag and drop (mouse / pen).
 *
 * - A fixed-position clone ("ghost") follows the pointer, lifted and tilted, swinging with momentum.
 * - The drop target is one primitive key `${columnId}:${beforeCardId | END}` read through
 *   createSelector, so hovering only flips two placeholders; siblings slide via FLIP.
 * - On release the ghost glides into the placeholder, then the real card takes its place.
 * - Esc or pointercancel glides the card back to where it started.
 */
const END = '__end';
const THRESHOLD = 5; // px of movement before a press becomes a drag (so clicks still open the card)
const EDGE = 64; // px from a scroll edge where auto-scroll kicks in

const [draggingId, setDraggingId] = createSignal<string | null>(null);
const [dropKey, setDropKey] = createSignal<string | null>(null);
const [ghostHeight, setGhostHeight] = createSignal(40);

export { draggingId, ghostHeight };
export const isDragging = createSelector(draggingId);
export const isDropTarget = createSelector(dropKey);
export const dropKeyFor = (columnId: string, beforeId: string | null) => `${columnId}:${beforeId ?? END}`;

function parseKey(key: string): { columnId: string; beforeId: string | null } {
  const sep = key.lastIndexOf(':');
  const before = key.slice(sep + 1);
  return { columnId: key.slice(0, sep), beforeId: before === END ? null : before };
}

interface Pending {
  id: string;
  el: HTMLElement;
  pointerId: number;
  startX: number;
  startY: number;
}

interface Drag {
  id: string;
  ghost: HTMLElement;
  originLeft: number;
  originTop: number;
  offX: number;
  offY: number;
  x: number;
  y: number;
  lastX: number;
  rot: number;
  scale: number;
  originKey: string;
  raf: number;
}

let pending: Pending | null = null;
let drag: Drag | null = null;

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

export function pointerDown(e: PointerEvent, id: string, el: HTMLElement) {
  if (e.button !== 0 || e.pointerType === 'touch' || pending || drag) return;
  pending = { id, el, pointerId: e.pointerId, startX: e.clientX, startY: e.clientY };
  window.addEventListener('pointermove', onMove);
  window.addEventListener('pointerup', onUp);
  window.addEventListener('pointercancel', onCancel);
}

function removeListeners() {
  window.removeEventListener('pointermove', onMove);
  window.removeEventListener('pointerup', onUp);
  window.removeEventListener('pointercancel', onCancel);
  window.removeEventListener('keydown', onKey, true);
}

function onMove(e: PointerEvent) {
  if (pending && e.pointerId === pending.pointerId) {
    if (Math.hypot(e.clientX - pending.startX, e.clientY - pending.startY) < THRESHOLD) return;
    begin(pending, e);
  }
  if (drag) {
    drag.x = e.clientX;
    drag.y = e.clientY;
  }
}

function onUp(e: PointerEvent) {
  if (pending) {
    // Just a click: let it through so the card opens.
    pending = null;
    removeListeners();
    return;
  }
  if (!drag) return;
  drag.x = e.clientX;
  drag.y = e.clientY;
  updateDropTarget(drag); // the last frame may not have run yet
  finish(true);
}

function onCancel() {
  if (pending) {
    pending = null;
    removeListeners();
  } else if (drag) finish(false);
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && drag) {
    e.preventDefault();
    e.stopImmediatePropagation();
    finish(false);
  }
}

function begin(p: Pending, e: PointerEvent) {
  const card = board.cards[p.id];
  if (!card) return;
  const r = p.el.getBoundingClientRect();

  const ghost = p.el.cloneNode(true) as HTMLElement;
  ghost.removeAttribute('data-card-id');
  ghost.removeAttribute('tabindex');
  ghost.classList.remove('card-flash', 'card-enter');
  ghost.classList.add('drag-ghost');
  Object.assign(ghost.style, {
    position: 'fixed',
    left: `${r.left}px`,
    top: `${r.top}px`,
    width: `${r.width}px`,
    height: `${r.height}px`,
    margin: '0',
    zIndex: '60',
    pointerEvents: 'none',
  });
  document.body.appendChild(ghost);

  const siblings = sortedCardsIn(card.column_id);
  const next = siblings[siblings.findIndex((c) => c.id === p.id) + 1];
  const originKey = dropKeyFor(card.column_id, next?.id ?? null);

  setGhostHeight(r.height);
  // The placeholder takes the card's exact slot, so nothing moves at pick-up.
  batch(() => {
    setDraggingId(p.id);
    setDropKey(originKey);
  });
  document.documentElement.classList.add('is-dragging');
  window.getSelection()?.removeAllRanges();

  drag = {
    id: p.id,
    ghost,
    originLeft: r.left,
    originTop: r.top,
    offX: p.startX - r.left,
    offY: p.startY - r.top,
    x: e.clientX,
    y: e.clientY,
    lastX: e.clientX,
    rot: 0,
    scale: 1,
    originKey,
    raf: 0,
  };
  pending = null;
  window.addEventListener('keydown', onKey, true);
  drag.raf = requestAnimationFrame(tick);
}

// ---------------------------------------------------------------------------
// Per-frame: move the ghost, swing it, auto-scroll, retarget
// ---------------------------------------------------------------------------

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

function tick() {
  const d = drag;
  if (!d) return;
  const still = prefersReducedMotion();
  const vx = d.x - d.lastX;
  d.lastX = d.x;
  // Base tilt of 3° (classic Trello), plus a swing in the direction of travel.
  const targetRot = still ? 0 : clamp(3 + vx * 0.6, -8, 12);
  d.rot += (targetRot - d.rot) * 0.2;
  d.scale += ((still ? 1 : 1.04) - d.scale) * 0.25;
  d.ghost.style.transform = `translate3d(${d.x - d.offX - d.originLeft}px, ${d.y - d.offY - d.originTop}px, 0) rotate(${d.rot.toFixed(2)}deg) scale(${d.scale.toFixed(3)})`;

  autoScroll(d);
  updateDropTarget(d);
  d.raf = requestAnimationFrame(tick);
}

function autoScroll(d: Drag) {
  const main = document.querySelector<HTMLElement>('main');
  if (main) {
    const r = main.getBoundingClientRect();
    if (d.x < r.left + EDGE) main.scrollLeft -= Math.ceil((r.left + EDGE - d.x) / 4);
    else if (d.x > r.right - EDGE) main.scrollLeft += Math.ceil((d.x - (r.right - EDGE)) / 4);
  }
  const list = columnAt(d.x)?.querySelector<HTMLElement>('[data-list]');
  if (list) {
    const r = list.getBoundingClientRect();
    if (d.y < r.top + EDGE / 2 && d.y > r.top - EDGE) list.scrollTop -= 8;
    else if (d.y > r.bottom - EDGE / 2 && d.y < r.bottom + EDGE) list.scrollTop += 8;
  }
}

function columnAt(x: number): HTMLElement | null {
  for (const col of document.querySelectorAll<HTMLElement>('[data-column-id]')) {
    const r = col.getBoundingClientRect();
    if (x >= r.left - 6 && x <= r.right + 6) return col;
  }
  return null;
}

function updateDropTarget(d: Drag) {
  const col = columnAt(d.x);
  if (!col) return; // between/outside columns: keep the last slot
  const list = col.querySelector<HTMLElement>('[data-list]');
  if (!list) return;

  // Layout positions (offsetTop ignores running FLIP transforms), so sliding cards can't
  // make the target flicker back and forth.
  const listTop = list.getBoundingClientRect().top - list.scrollTop;
  let before: string | null = null;
  for (const el of list.querySelectorAll<HTMLElement>('[data-card-id]')) {
    const id = el.dataset.cardId!;
    if (id === d.id) continue;
    if (d.y < listTop + el.offsetTop + el.offsetHeight / 2) {
      before = id;
      break;
    }
  }
  const key = dropKeyFor(col.dataset.columnId!, before);
  const prev = dropKey();
  if (key === prev) return;
  // Siblings in the old and new column slide to make room.
  flip(document, () => setDropKey(key), { exclude: d.id, duration: 180 });
}

// ---------------------------------------------------------------------------
// Drop
// ---------------------------------------------------------------------------

function finish(commit: boolean) {
  const d = drag;
  if (!d) return;
  cancelAnimationFrame(d.raf);
  removeListeners();
  if (!commit && dropKey() !== d.originKey) {
    flip(document, () => setDropKey(d.originKey), { exclude: d.id, duration: 180 });
  }
  const key = dropKey();

  const done = () => {
    batch(() => {
      if (commit && key) applyDrop(d.id, key);
      setDraggingId(null);
      setDropKey(null);
    });
    d.ghost.remove();
    document.documentElement.classList.remove('is-dragging');
    drag = null;
  };

  // A click can follow pointerup; don't let it open whatever is under the cursor.
  const swallow = (ev: Event) => ev.stopPropagation();
  window.addEventListener('click', swallow, { capture: true, once: true });
  setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0);

  const slot = document.querySelector<HTMLElement>('[data-placeholder]')?.getBoundingClientRect();
  if (!slot || prefersReducedMotion()) return done();

  d.ghost.classList.add('drag-ghost-drop');
  const anim = d.ghost.animate(
    [
      { transform: d.ghost.style.transform },
      { transform: `translate3d(${slot.left - d.originLeft}px, ${slot.top - d.originTop}px, 0) rotate(0deg) scale(1)` },
    ],
    { duration: 240, easing: 'cubic-bezier(.2,.9,.3,1.12)', fill: 'forwards' },
  );
  anim.onfinish = done;
  anim.oncancel = done;
}

/** Turn the drop slot into a position between the neighbours and move the card (optimistic). */
function applyDrop(id: string, key: string) {
  const card = board.cards[id];
  if (!card) return;
  const { columnId, beforeId } = parseKey(key);
  const list = sortedCardsIn(columnId).filter((c) => c.id !== id);
  const idx = beforeId ? list.findIndex((c) => c.id === beforeId) : list.length;
  if (idx < 0) return;
  const prev = list[idx - 1];
  const next = list[idx];

  // Dropped where it already was: nothing to save.
  if (card.column_id === columnId) {
    const origIdx = sortedCardsIn(columnId).findIndex((c) => c.id === id);
    if (list[origIdx]?.id === next?.id) return;
  }

  const position =
    prev && next ? (prev.position + next.position) / 2 : prev ? prev.position + GAP : next ? next.position - GAP : GAP;
  void moveCard(id, columnId, position);
}
