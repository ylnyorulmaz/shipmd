import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { apiOrigin } from '../api';
import { renderMarkdown } from '../markdown';
import { board, deleteCard, setOpenCardId, toggleAssignee, updateCard } from '../store';
import { TAGS } from '../types';
import { AssigneeBadge, TAG_LABEL, TAG_SOLID } from '../ui';

function timeAgo(iso: string): string {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 45) return 'just now';
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  return new Date(iso).toLocaleDateString();
}

export default function CardModal(props: { id: string }) {
  const card = () => board.cards[props.id];
  const column = createMemo(() => board.columns.find((c) => c.id === card()?.column_id));

  const [editing, setEditing] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  const [copied, setCopied] = createSignal<string | null>(null);
  let titleEl!: HTMLTextAreaElement;
  let descEl: HTMLTextAreaElement | undefined;

  const shortId = () => props.id.slice(0, 8);
  const html = createMemo(() => (card()?.description ? renderMarkdown(card()!.description!) : ''));

  const fitTitle = () => {
    if (!titleEl) return;
    titleEl.style.height = 'auto';
    titleEl.style.height = `${titleEl.scrollHeight}px`;
  };
  onMount(fitTitle);
  // Keep the title box in sync when an agent renames the card while it's open.
  createEffect(
    on(
      () => card()?.title,
      (t) => {
        if (t !== undefined && document.activeElement !== titleEl) {
          titleEl.value = t;
          fitTitle();
        }
      },
    ),
  );

  const saveTitle = () => {
    const t = titleEl.value.replace(/\s+/g, ' ').trim();
    if (!t) titleEl.value = card()!.title;
    else if (t !== card()!.title) void updateCard(props.id, { title: t });
  };

  const startEdit = () => {
    setDraft(card()?.description ?? '');
    setEditing(true);
    queueMicrotask(() => {
      descEl?.focus();
      descEl?.setSelectionRange(descEl.value.length, descEl.value.length);
    });
  };
  const saveDesc = () => {
    if (!editing()) return;
    setEditing(false);
    if (draft() !== (card()?.description ?? '')) void updateCard(props.id, { description: draft() });
  };

  const close = () => {
    saveTitle();
    saveDesc();
    setOpenCardId(null);
  };

  // Escape closes (saving any in-progress edits); handled here so it wins over the global handler.
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopImmediatePropagation();
      close();
    }
  };
  window.addEventListener('keydown', onKey, true);
  onCleanup(() => window.removeEventListener('keydown', onKey, true));

  const copy = async (label: string, text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(label);
      setTimeout(() => setCopied(null), 1400);
    } catch {
      /* clipboard unavailable */
    }
  };
  const agentCurl = () =>
    `curl -s -X POST ${apiOrigin()}/api/agent/action -H 'content-type: application/json' ` +
    `-d '{"card_id":"${shortId()}","action":"complete","note":"Shipped it."}'`;

  return (
    <Show when={card()}>
      <div
        class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-slate-900/50 px-4 py-12 backdrop-blur-[1px] dark:bg-black/60"
        onMouseDown={(e) => {
          if (e.target === e.currentTarget) close();
        }}
      >
        <div
          role="dialog"
          aria-modal="true"
          class="modal-in relative w-full max-w-[640px] rounded-lg bg-[#f4f5f7] p-5 shadow-2xl dark:bg-zinc-900 dark:ring-1 dark:ring-zinc-800"
        >
          <button
            type="button"
            aria-label="Close"
            onClick={close}
            class="absolute top-3 right-3 grid h-8 w-8 place-items-center rounded-md text-xl text-slate-500 hover:bg-black/5 hover:text-slate-800 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-zinc-100"
          >
            ×
          </button>

          <textarea
            ref={titleEl}
            rows={1}
            value={card()!.title}
            onInput={fitTitle}
            onBlur={saveTitle}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                titleEl.blur();
              }
            }}
            class="-mx-1.5 block w-[calc(100%-2rem)] resize-none overflow-hidden rounded-[4px] border-0 bg-transparent px-1.5 py-1 text-xl font-semibold text-slate-800 outline-none focus:bg-white focus:ring-2 focus:ring-sky-500 dark:text-zinc-100 dark:focus:bg-zinc-800"
          />
          <p class="mt-0.5 text-[13px] text-slate-500 dark:text-zinc-400">
            in list <span class="font-medium text-slate-700 underline decoration-dotted underline-offset-2 dark:text-zinc-300">{column()?.title}</span>
          </p>

          {/* Meta row */}
          <div class="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-3">
            <div>
              <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">Tag</div>
              <div class="flex gap-1">
                <For each={TAGS}>
                  {(t) => (
                    <button
                      type="button"
                      onClick={() => card()!.tag !== t && updateCard(props.id, { tag: t })}
                      class="h-7 rounded-[3px] px-2.5 text-[11px] font-semibold tracking-wide uppercase transition-colors"
                      classList={{
                        [TAG_SOLID[t]]: card()!.tag === t,
                        'bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10': card()!.tag !== t,
                      }}
                    >
                      {TAG_LABEL[t]}
                    </button>
                  )}
                </For>
              </div>
            </div>
            <div>
              <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">Assignee</div>
              <AssigneeBadge size="md" assignee={card()!.assignee} onToggle={() => toggleAssignee(props.id)} />
            </div>
            <div>
              <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">List</div>
              <select
                value={card()!.column_id}
                onChange={(e) => updateCard(props.id, { column_id: e.currentTarget.value })}
                class="h-7 w-full rounded-[4px] border border-slate-300 bg-white px-1.5 text-sm text-slate-700 outline-none focus:ring-2 focus:ring-sky-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200"
              >
                <For each={board.columns}>{(c) => <option value={c.id}>{c.title}</option>}</For>
              </select>
            </div>
          </div>

          {/* Description */}
          <div class="mt-6">
            <div class="mb-2 flex items-center justify-between">
              <h3 class="text-[15px] font-semibold text-slate-800 dark:text-zinc-100">Description</h3>
              <Show when={!editing() && card()!.description}>
                <button
                  type="button"
                  onClick={startEdit}
                  class="h-7 rounded-[4px] bg-black/5 px-3 text-[13px] text-slate-700 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-300 dark:hover:bg-white/10"
                >
                  Edit
                </button>
              </Show>
            </div>

            <Show
              when={editing()}
              fallback={
                <Show
                  when={card()!.description}
                  fallback={
                    <button
                      type="button"
                      onClick={startEdit}
                      class="block min-h-[64px] w-full rounded-[4px] bg-black/5 px-3 py-2.5 text-left text-sm text-slate-500 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10"
                    >
                      Add a more detailed description… <span class="opacity-60">(Markdown supported)</span>
                    </button>
                  }
                >
                  <div class="md cursor-text" onDblClick={startEdit} innerHTML={html()} />
                </Show>
              }
            >
              <textarea
                ref={descEl}
                value={draft()}
                onInput={(e) => setDraft(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                    e.preventDefault();
                    saveDesc();
                  }
                }}
                rows={9}
                placeholder="Markdown: **bold**, - [ ] tasks, `code`, links…"
                class="block w-full resize-y rounded-[4px] border border-slate-300 bg-white px-3 py-2 font-mono text-[13px] leading-relaxed text-slate-800 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
              />
              <div class="mt-2 flex items-center gap-2">
                <button
                  type="button"
                  onClick={saveDesc}
                  class="h-8 rounded-[4px] bg-sky-600 px-3 text-sm font-medium text-white hover:bg-sky-700"
                >
                  Save
                </button>
                <button
                  type="button"
                  onClick={() => setEditing(false)}
                  class="h-8 rounded-[4px] px-3 text-sm text-slate-600 hover:bg-black/5 dark:text-zinc-400 dark:hover:bg-white/10"
                >
                  Cancel
                </button>
                <span class="ml-auto text-[11px] text-slate-400 dark:text-zinc-500">⌘/Ctrl + ↵ to save</span>
              </div>
            </Show>
          </div>

          {/* Footer */}
          <div class="mt-6 flex flex-wrap items-center gap-2 border-t border-slate-200 pt-4 text-xs text-slate-500 dark:border-zinc-800 dark:text-zinc-500">
            <button
              type="button"
              title="Copy card id (agents can use this short id)"
              onClick={() => copy('id', shortId())}
              class="rounded bg-black/5 px-1.5 py-0.5 font-mono hover:bg-black/10 dark:bg-white/5 dark:hover:bg-white/10"
            >
              {copied() === 'id' ? 'copied ✓' : `#${shortId()}`}
            </button>
            <span>updated {timeAgo(card()!.updated_at)}</span>
            <button
              type="button"
              onClick={() => copy('curl', agentCurl())}
              title={agentCurl()}
              class="rounded px-1.5 py-0.5 hover:bg-black/5 hover:text-slate-700 dark:hover:bg-white/10 dark:hover:text-zinc-300"
            >
              {copied() === 'curl' ? 'copied ✓' : '🤖 Copy agent curl'}
            </button>
            <div class="ml-auto">
              <Show
                when={confirmDelete()}
                fallback={
                  <button
                    type="button"
                    onClick={() => setConfirmDelete(true)}
                    class="h-8 rounded-[4px] px-3 text-sm text-slate-500 hover:bg-red-50 hover:text-red-600 dark:text-zinc-400 dark:hover:bg-red-500/10 dark:hover:text-red-400"
                  >
                    Delete
                  </button>
                }
              >
                <button
                  type="button"
                  onClick={() => setConfirmDelete(false)}
                  class="mr-1 h-8 rounded-[4px] px-3 text-sm text-slate-500 hover:bg-black/5 dark:text-zinc-400 dark:hover:bg-white/10"
                >
                  Keep
                </button>
                <button
                  type="button"
                  onClick={() => deleteCard(props.id)}
                  class="h-8 rounded-[4px] bg-red-600 px-3 text-sm font-medium text-white hover:bg-red-700"
                >
                  Delete card
                </button>
              </Show>
            </div>
          </div>
        </div>
      </div>
    </Show>
  );
}
