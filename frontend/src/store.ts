import { createSignal } from 'solid-js';
import { createStore, produce, reconcile } from 'solid-js/store';
import { api, CLIENT_ID, uuid, wsUrl } from './api';
import { flip, prefersReducedMotion } from './flip';
import type { Assignee, Card, CardPatch, Column, Tag, WsEvent } from './types';

export const GAP = 1024;

interface BoardState {
  title: string;
  columns: Column[];
  cards: Record<string, Card>;
  loaded: boolean;
}

export const [board, setBoard] = createStore<BoardState>({
  title: 'Ship Board',
  columns: [],
  cards: {},
  loaded: false,
});

// ---------------------------------------------------------------------------
// UI state
// ---------------------------------------------------------------------------

export const [connected, setConnected] = createSignal(false);
export const [openCardId, setOpenCardId] = createSignal<string | null>(null);
/** Column whose inline "add card" composer is open. */
export const [composerColumn, setComposerColumn] = createSignal<string | null>(null);

export const [toast, setToast] = createSignal<string | null>(null);
let toastTimer: number | undefined;
export function showToast(msg: string) {
  setToast(msg);
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => setToast(null), 3500);
}

// ---------------------------------------------------------------------------
// Selectors
// ---------------------------------------------------------------------------

export function sortedCardsIn(columnId: string): Card[] {
  const out: Card[] = [];
  for (const c of Object.values(board.cards)) if (c.column_id === columnId) out.push(c);
  return out.sort((a, b) => a.position - b.position);
}

export const doneColumnId = () => board.columns[board.columns.length - 1]?.id;

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

export async function loadBoard() {
  try {
    const res = await api.board();
    const cards: Record<string, Card> = {};
    for (const col of res.columns) for (const c of col.cards) cards[c.id] = c;
    setBoard('title', res.title);
    setBoard('columns', reconcile(res.columns.map(({ cards: _, ...col }) => col), { key: 'id' }));
    setBoard('cards', reconcile(cards));
    setBoard('loaded', true);
    document.title = res.title;
  } catch (e) {
    showToast(`Could not load board: ${(e as Error).message}`);
  }
}

// ---------------------------------------------------------------------------
// Optimistic mutations: update the store first, sync in the background,
// roll back on failure.
// ---------------------------------------------------------------------------

export async function moveCard(id: string, column_id: string, position: number) {
  const card = board.cards[id];
  if (!card) return;
  const prev = { column_id: card.column_id, position: card.position };
  setBoard('cards', id, { column_id, position });
  try {
    const saved = await api.patch(id, { column_id, position });
    setBoard('cards', id, 'updated_at', saved.updated_at);
  } catch (e) {
    if (board.cards[id]) setBoard('cards', id, prev);
    showToast(`Move failed: ${(e as Error).message}`);
  }
}

export async function updateCard(id: string, patch: CardPatch) {
  const card = board.cards[id];
  if (!card) return;
  const prev: Partial<Card> = {};
  for (const k of Object.keys(patch) as (keyof CardPatch)[]) (prev as any)[k] = card[k];
  const local: Partial<Card> = { ...patch } as Partial<Card>;
  if (patch.description !== undefined) local.description = patch.description.trim() ? patch.description : null;
  setBoard('cards', id, local);
  try {
    const saved = await api.patch(id, patch);
    setBoard('cards', id, 'updated_at', saved.updated_at);
  } catch (e) {
    if (board.cards[id]) setBoard('cards', id, prev);
    showToast(`Update failed: ${(e as Error).message}`);
  }
}

export function toggleAssignee(id: string) {
  const card = board.cards[id];
  if (!card) return;
  const next: Assignee = card.assignee === 'agent' ? 'human' : 'agent';
  return updateCard(id, { assignee: next });
}

export async function createCard(input: { column_id: string; title: string; tag: Tag; assignee: Assignee }) {
  const last = sortedCardsIn(input.column_id).at(-1);
  const card: Card = {
    id: uuid(),
    column_id: input.column_id,
    title: input.title,
    description: null,
    tag: input.tag,
    assignee: input.assignee,
    position: last ? last.position + GAP : GAP,
    updated_at: new Date().toISOString(),
  };
  setBoard('cards', card.id, card);
  try {
    const { updated_at: _, ...body } = card;
    const saved = await api.create(body);
    if (board.cards[card.id]) setBoard('cards', card.id, saved);
  } catch (e) {
    removeLocal(card.id);
    showToast(`Could not create card: ${(e as Error).message}`);
  }
}

export async function deleteCard(id: string) {
  const card = board.cards[id];
  if (!card) return;
  const snapshot = { ...card };
  if (openCardId() === id) setOpenCardId(null);
  removeLocal(id);
  try {
    await api.remove(id);
  } catch (e) {
    setBoard('cards', id, snapshot);
    showToast(`Delete failed: ${(e as Error).message}`);
  }
}

function removeLocal(id: string) {
  setBoard(
    'cards',
    produce((cards) => {
      delete cards[id];
    }),
  );
}

// ---------------------------------------------------------------------------
// Real-time sync
// ---------------------------------------------------------------------------

const cardEl = (id: string) => document.querySelector<HTMLElement>(`[data-card-id="${CSS.escape(id)}"]`);

/** Play the highlight; restarts it if the card is still flashing from an earlier update. */
function flash(el: HTMLElement) {
  el.classList.remove('card-flash');
  void el.offsetWidth; // force reflow so the CSS animation starts over
  el.classList.add('card-flash');
}

/**
 * FLIP-style glide for moves that come from elsewhere (an agent, curl, another tab).
 * A fixed-position clone flies from the old spot to the new one, so it is never
 * clipped by a column's scroll container.
 */
function animateRemoteMove(id: string, mutate: () => void) {
  const before = cardEl(id);
  const first = before?.getBoundingClientRect();
  const ghost = before?.cloneNode(true) as HTMLElement | undefined;
  flip(document, mutate, { exclude: id }); // siblings slide; Solid updates the DOM synchronously
  const after = cardEl(id);
  if (!after) return;
  if (!first || !ghost || prefersReducedMotion()) {
    flash(after);
    return;
  }
  const last = after.getBoundingClientRect();
  if (Math.abs(first.left - last.left) < 1 && Math.abs(first.top - last.top) < 1) {
    flash(after);
    return;
  }
  Object.assign(ghost.style, {
    position: 'fixed',
    left: `${first.left}px`,
    top: `${first.top}px`,
    width: `${first.width}px`,
    margin: '0',
    zIndex: '60',
    pointerEvents: 'none',
  });
  ghost.removeAttribute('data-card-id');
  document.body.appendChild(ghost);
  after.style.visibility = 'hidden';
  const anim = ghost.animate(
    [
      { transform: 'translate(0,0) rotate(0deg)', boxShadow: '0 1px 0 rgba(9,30,66,.25)' },
      {
        transform: `translate(${(last.left - first.left) * 0.5}px, ${(last.top - first.top) * 0.5 - 12}px) rotate(2deg)`,
        boxShadow: '0 12px 24px rgba(9,30,66,.25)',
        offset: 0.5,
      },
      { transform: `translate(${last.left - first.left}px, ${last.top - first.top}px) rotate(0deg)`, boxShadow: '0 1px 0 rgba(9,30,66,.25)' },
    ],
    { duration: 520, easing: 'cubic-bezier(.2,.8,.2,1)' },
  );
  const finish = () => {
    ghost.remove();
    after.style.visibility = '';
    flash(after);
  };
  anim.onfinish = finish;
  anim.oncancel = finish;
}

function applyEvent(ev: WsEvent) {
  switch (ev.type) {
    case 'HELLO':
      return;
    case 'RESYNC':
      void loadBoard();
      return;
    case 'CARD_DELETED':
      if (ev.origin === CLIENT_ID) return;
      if (openCardId() === ev.id) setOpenCardId(null);
      flip(document, () => removeLocal(ev.id));
      return;
    case 'CARD_CREATED':
    case 'CARD_MOVED':
    case 'CARD_UPDATED': {
      if (ev.origin === CLIENT_ID) return;
      const existing = board.cards[ev.card.id];
      if (!existing) {
        flip(document, () => setBoard('cards', ev.card.id, ev.card));
        requestAnimationFrame(() => cardEl(ev.card.id)?.classList.add('card-enter'));
        return;
      }
      const moved = existing.column_id !== ev.card.column_id || existing.position !== ev.card.position;
      if (moved) animateRemoteMove(ev.card.id, () => setBoard('cards', ev.card.id, ev.card));
      else setBoard('cards', ev.card.id, ev.card);
      return;
    }
  }
}

export function connectSocket() {
  let retry = 0;
  let firstConnect = true;

  const open = () => {
    const ws = new WebSocket(wsUrl());
    ws.onopen = () => {
      retry = 0;
      setConnected(true);
      // After a reconnect we may have missed events: refetch.
      if (!firstConnect) void loadBoard();
      firstConnect = false;
    };
    ws.onmessage = (msg) => {
      try {
        applyEvent(JSON.parse(msg.data) as WsEvent);
      } catch (e) {
        console.warn('bad ws message', e);
      }
    };
    ws.onclose = () => {
      setConnected(false);
      const delay = Math.min(10_000, 500 * 2 ** retry++);
      setTimeout(open, delay);
    };
    ws.onerror = () => ws.close();
  };
  open();
}
