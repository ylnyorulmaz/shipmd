import { createMemo, For, Show } from 'solid-js';
import { dragOverColumn, dropKeyFor, dropOnColumn, ghostHeight, isDropTarget } from '../dnd';
import { composerColumn, setComposerColumn, sortedCardsIn } from '../store';
import type { Column } from '../types';
import CardItem from './CardItem';
import Composer from './Composer';

function Placeholder() {
  return (
    <div
      class="mb-2 rounded-[4px] bg-black/[.08] dark:bg-white/[.06]"
      style={{ height: `${ghostHeight()}px` }}
    />
  );
}

export default function BoardColumn(props: { column: Column }) {
  let listEl!: HTMLDivElement;
  const cards = createMemo(() => sortedCardsIn(props.column.id));
  const composing = () => composerColumn() === props.column.id;

  return (
    <section
      class="flex max-h-full w-[272px] shrink-0 flex-col rounded-md border border-[#dfe1e6] bg-[#ebecf0] dark:border-zinc-800 dark:bg-zinc-900"
      onDragOver={(e) => dragOverColumn(e, props.column.id, listEl)}
      onDrop={dropOnColumn}
    >
      <header class="flex items-center justify-between px-3 pt-2.5 pb-1.5">
        <h2 class="text-[14px] font-semibold text-slate-800 dark:text-zinc-100">{props.column.title}</h2>
        <span class="rounded-full px-1.5 text-xs font-medium text-slate-500 tabular-nums dark:text-zinc-500">{cards().length}</span>
      </header>

      <div ref={listEl} class="min-h-[8px] flex-1 overflow-y-auto px-2 pt-0.5 pb-0.5 [scrollbar-width:thin]">
        <For each={cards()}>
          {(card) => (
            <>
              <Show when={isDropTarget(dropKeyFor(props.column.id, card.id))}>
                <Placeholder />
              </Show>
              <CardItem card={card} />
            </>
          )}
        </For>
        <Show when={isDropTarget(dropKeyFor(props.column.id, null))}>
          <Placeholder />
        </Show>
        <Show when={composing()}>
          <Composer columnId={props.column.id} />
        </Show>
      </div>

      <Show when={!composing()}>
        <button
          type="button"
          onClick={() => setComposerColumn(props.column.id)}
          class="mx-2 mt-0.5 mb-2 flex h-8 items-center gap-1.5 rounded-[4px] px-2 text-left text-sm text-slate-500 hover:bg-black/[.07] hover:text-slate-800 dark:text-zinc-400 dark:hover:bg-white/[.06] dark:hover:text-zinc-100"
        >
          <span class="text-base leading-none">+</span> Add a card
        </button>
      </Show>
      <Show when={composing()}>
        <div class="h-2" />
      </Show>
    </section>
  );
}
