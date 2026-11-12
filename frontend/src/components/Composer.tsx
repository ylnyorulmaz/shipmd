import { createSignal, For, onMount } from 'solid-js';
import { createCard, setComposerColumn } from '../store';
import { TAGS, type Assignee, type Tag } from '../types';
import { TAG_LABEL, TAG_SOLID } from '../ui';

/** Inline Trello-style "add a card" composer. Supports `#build #sell #ops` and `@agent` tokens. */
export default function Composer(props: { columnId: string }) {
  let input!: HTMLTextAreaElement;
  const [tag, setTag] = createSignal<Tag>(props.columnId === 'sell' ? 'sell' : 'build');
  const [assignee, setAssignee] = createSignal<Assignee>('human');

  onMount(() => {
    input.focus();
    input.scrollIntoView({ block: 'nearest' });
  });

  const submit = () => {
    let title = input.value;
    let t = tag();
    let a = assignee();
    title = title.replace(/(^|\s)#(build|sell|ops)\b/gi, (_, sp, m: string) => ((t = m.toLowerCase() as Tag), sp));
    title = title.replace(/(^|\s)@(agent|bot|ai)\b/gi, (_, sp) => ((a = 'agent'), sp));
    title = title.replace(/\s+/g, ' ').trim();
    if (!title) return;
    void createCard({ column_id: props.columnId, title, tag: t, assignee: a });
    input.value = '';
    input.style.height = '';
    input.focus();
  };

  return (
    <div class="mb-1" onClick={(e) => e.stopPropagation()}>
      <textarea
        ref={input}
        rows={2}
        placeholder="Enter a title… (#sell, @agent)"
        class="block w-full resize-none rounded-[4px] bg-white px-2.5 py-2 text-[14px] leading-[1.35] text-slate-800 placeholder:text-slate-400
               shadow-[0_1px_0_rgba(9,30,66,.25)] outline-none focus:ring-2 focus:ring-sky-500
               dark:bg-zinc-800 dark:text-zinc-100 dark:placeholder:text-zinc-500"
        onInput={(e) => {
          e.currentTarget.style.height = 'auto';
          e.currentTarget.style.height = `${e.currentTarget.scrollHeight}px`;
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
            e.preventDefault();
            submit();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            setComposerColumn(null);
          }
        }}
      />
      <div class="mt-2 flex flex-wrap items-center gap-1.5">
        <For each={TAGS}>
          {(t) => (
            <button
              type="button"
              onClick={() => setTag(t)}
              class="h-6 rounded-[3px] px-2 text-[11px] font-semibold uppercase tracking-wide transition-colors"
              classList={{
                [TAG_SOLID[t]]: tag() === t,
                'bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10': tag() !== t,
              }}
            >
              {TAG_LABEL[t]}
            </button>
          )}
        </For>
        <button
          type="button"
          onClick={() => setAssignee((a) => (a === 'agent' ? 'human' : 'agent'))}
          class="h-6 rounded-full px-2 text-[11px] font-medium transition-colors"
          classList={{
            'bg-violet-600 text-white': assignee() === 'agent',
            'bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10': assignee() === 'human',
          }}
        >
          {assignee() === 'agent' ? '🤖 Agent' : '👤 Me'}
        </button>
      </div>
      <div class="mt-2 flex items-center gap-2">
        <button
          type="button"
          onClick={submit}
          class="h-8 rounded-[4px] bg-sky-600 px-3 text-sm font-medium text-white shadow-sm hover:bg-sky-700 active:bg-sky-800"
        >
          Add card
        </button>
        <button
          type="button"
          aria-label="Cancel"
          onClick={() => setComposerColumn(null)}
          class="grid h-8 w-8 place-items-center rounded-[4px] text-xl leading-none text-slate-500 hover:bg-black/5 hover:text-slate-800 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-zinc-100"
        >
          ×
        </button>
        <span class="ml-auto text-[11px] text-slate-400 dark:text-zinc-500">↵ add · esc close</span>
      </div>
    </div>
  );
}
