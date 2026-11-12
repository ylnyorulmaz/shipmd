import { createSelector, createSignal } from 'solid-js';
import { board, GAP, moveCard, sortedCardsIn } from './store';

/**
 * Native HTML5 drag-and-drop.
 *
 * Drop target is encoded as a primitive key `${columnId}:${beforeCardId | END}` so that
 * hovering only flips two `<Show>` placeholders (old & new) via createSelector —
 * no list or card re-renders while dragging.
 */
const END = '__end';

const [draggingId, setDraggingId] = createSignal<string | null>(null);
const [dropKey, setDropKey] = createSignal<string | null>(null);
const [ghostHeight, setGhostHeight] = createSignal(40);

export { draggingId, ghostHeight };
export const isDragging = createSelector(draggingId);
export const isDropTarget = createSelector(dropKey);
export const dropKeyFor = (columnId: string, beforeId: string | null) => `${columnId}:${beforeId ?? END}`;

export function startDrag(e: DragEvent, cardId: string, el: HTMLElement) {
  e.dataTransfer?.setData('text/plain', cardId);
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  setGhostHeight(el.offsetHeight);
  const card = board.cards[cardId];
  const siblings = sortedCardsIn(card.column_id);
  const next = siblings[siblings.findIndex((c) => c.id === cardId) + 1];
  // Defer hiding so the browser snapshots the card for the drag image first.
  requestAnimationFrame(() => {
    setDraggingId(cardId);
    setDropKey(dropKeyFor(card.column_id, next?.id ?? null));
  });
}

export function endDrag() {
  setDraggingId(null);
  setDropKey(null);
}

/** Called on dragover anywhere in a column; picks the slot from the pointer's Y. */
export function dragOverColumn(e: DragEvent, columnId: string, listEl: HTMLElement) {
  const dragging = draggingId();
  if (!dragging) return; // ignore foreign drags (files, text)
  e.preventDefault();
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';

  let before: string | null = null;
  for (const el of listEl.querySelectorAll<HTMLElement>('[data-card-id]')) {
    const id = el.dataset.cardId!;
    if (id === dragging) continue;
    const r = el.getBoundingClientRect();
    if (e.clientY < r.top + r.height / 2) {
      before = id;
      break;
    }
  }
  setDropKey(dropKeyFor(columnId, before));
}

export function dropOnColumn(e: DragEvent) {
  const id = draggingId();
  const key = dropKey();
  endDrag();
  if (!id || !key) return;
  e.preventDefault();

  const sep = key.lastIndexOf(':');
  const columnId = key.slice(0, sep);
  const beforeId = key.slice(sep + 1) === END ? null : key.slice(sep + 1);

  const list = sortedCardsIn(columnId).filter((c) => c.id !== id);
  const idx = beforeId ? list.findIndex((c) => c.id === beforeId) : list.length;
  if (idx < 0) return;
  const prev = list[idx - 1];
  const next = list[idx];

  const card = board.cards[id];
  if (!card) return;
  // No-op if it lands where it already was.
  if (card.column_id === columnId) {
    const orig = sortedCardsIn(columnId).filter((c) => c.id !== id);
    const origIdx = sortedCardsIn(columnId).findIndex((c) => c.id === id);
    if (orig[origIdx]?.id === next?.id) return;
  }

  const position =
    prev && next ? (prev.position + next.position) / 2 : prev ? prev.position + GAP : next ? next.position - GAP : GAP;
  void moveCard(id, columnId, position);
}
