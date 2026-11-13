import { Show } from 'solid-js';
import { isDragging, pointerDown } from '../dnd';
import { setOpenCardId, toggleAssignee } from '../store';
import type { Card } from '../types';
import { AssigneeBadge, TagPill } from '../ui';

export default function CardItem(props: { card: Card }) {
  let el!: HTMLDivElement;
  const open = () => setOpenCardId(props.card.id);

  return (
    <div
      ref={el}
      data-card-id={props.card.id}
      tabIndex={0}
      onPointerDown={(e) => pointerDown(e, props.card.id, el)}
      onClick={open}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && e.target === el) open();
      }}
      class="card group mb-2 cursor-pointer select-none rounded-[4px] bg-white px-2.5 pt-2 pb-2 outline-none
             shadow-[0_1px_0_rgba(9,30,66,.25)] hover:bg-slate-50 focus-visible:ring-2 focus-visible:ring-sky-500
             dark:bg-zinc-800 dark:shadow-[0_1px_0_rgba(0,0,0,.6)] dark:hover:bg-[#2a2d33]"
      classList={{ hidden: isDragging(props.card.id) }}
    >
      <div class="mb-1.5 flex items-center gap-1">
        <TagPill tag={props.card.tag} />
      </div>
      <div class="text-[14px] leading-[1.35] break-words text-slate-800 dark:text-zinc-100">{props.card.title}</div>
      <div class="mt-2 flex items-center justify-between">
        <AssigneeBadge assignee={props.card.assignee} onToggle={() => toggleAssignee(props.card.id)} />
        <Show when={props.card.description}>
          <svg class="h-3.5 w-3.5 text-slate-400 dark:text-zinc-500" viewBox="0 0 16 16" fill="currentColor" aria-label="Has description">
            <rect x="2" y="3" width="12" height="1.6" rx=".8" />
            <rect x="2" y="7.2" width="12" height="1.6" rx=".8" />
            <rect x="2" y="11.4" width="8" height="1.6" rx=".8" />
          </svg>
        </Show>
      </div>
    </div>
  );
}
