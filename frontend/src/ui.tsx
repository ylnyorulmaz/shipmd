import type { Assignee, Tag } from './types';

export const TAG_LABEL: Record<Tag, string> = { build: 'Build', sell: 'Sell', ops: 'Ops' };

export const TAG_PILL: Record<Tag, string> = {
  build: 'bg-sky-100 text-sky-800 dark:bg-sky-500/15 dark:text-sky-300',
  sell: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-500/15 dark:text-emerald-300',
  ops: 'bg-amber-100 text-amber-800 dark:bg-amber-500/15 dark:text-amber-300',
};

export const TAG_SOLID: Record<Tag, string> = {
  build: 'bg-sky-600 text-white',
  sell: 'bg-emerald-600 text-white',
  ops: 'bg-amber-500 text-white',
};

export function TagPill(props: { tag: Tag }) {
  return (
    <span
      class={`inline-flex h-[18px] items-center rounded-[3px] px-1.5 text-[11px] font-semibold uppercase tracking-wide ${TAG_PILL[props.tag]}`}
    >
      {TAG_LABEL[props.tag]}
    </span>
  );
}

export const assigneeLabel = (a: Assignee) => (a === 'agent' ? '🤖 Agent' : '👤 Me');

export function AssigneeBadge(props: { assignee: Assignee; onToggle: () => void; size?: 'sm' | 'md' }) {
  return (
    <button
      type="button"
      title={props.assignee === 'agent' ? 'Assigned to AI agent — click to take it back' : 'Assigned to you — click to hand to agent'}
      onClick={(e) => {
        e.stopPropagation();
        props.onToggle();
      }}
      onMouseDown={(e) => e.stopPropagation()}
      draggable={false}
      class="inline-flex cursor-pointer select-none items-center gap-1 rounded-full border font-medium transition-colors"
      classList={{
        'h-[22px] px-2 text-[11px]': props.size !== 'md',
        'h-7 px-3 text-xs': props.size === 'md',
        'border-violet-200 bg-violet-50 text-violet-700 hover:bg-violet-100 dark:border-violet-400/25 dark:bg-violet-500/10 dark:text-violet-300 dark:hover:bg-violet-500/20':
          props.assignee === 'agent',
        'border-slate-200 bg-white text-slate-600 hover:bg-slate-100 dark:border-zinc-600 dark:bg-zinc-700/40 dark:text-zinc-300 dark:hover:bg-zinc-700':
          props.assignee === 'human',
      }}
    >
      {assigneeLabel(props.assignee)}
    </button>
  );
}
