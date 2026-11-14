import { createMemo, For, Show } from 'solid-js';
import { assigneeFilter, board, setAssigneeFilter } from '../store';
import { ASSIGNEE_PRESETS } from '../types';
import { assigneeDisplay, isBuiltinAssignee, normalizeAssignee } from '../ui';

export default function AssigneeFilterBar() {
  const stats = createMemo(() => {
    const counts = new Map<string, number>();
    let total = 0;
    for (const c of Object.values(board.cards)) {
      const key = normalizeAssignee(c.assignee) || 'human';
      counts.set(key, (counts.get(key) ?? 0) + 1);
      total++;
    }

    const customKeys = new Set<string>();
    for (const k of counts.keys()) {
      if (!isBuiltinAssignee(k)) customKeys.add(k);
    }
    const active = assigneeFilter();
    if (active && !isBuiltinAssignee(active)) customKeys.add(active);

    const options: string[] = [...ASSIGNEE_PRESETS, ...[...customKeys].sort()];
    return { counts, total, options };
  });

  return (
    <Show when={board.loaded}>
      <div
        aria-label="Filter by assignee"
        class="flex shrink-0 items-center gap-1.5 overflow-x-auto border-b border-slate-200/80 bg-white/75 px-4 py-1.5 [scrollbar-width:none] dark:border-zinc-800/80 dark:bg-zinc-950/75"
      >
        <span class="mr-1 shrink-0 text-[11px] font-semibold tracking-wide text-slate-400 uppercase dark:text-zinc-500">
          Assignee
        </span>

        <button
          type="button"
          onClick={() => setAssigneeFilter(null)}
          aria-pressed={assigneeFilter() === null}
          class={`inline-flex h-6 shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-[11px] font-medium transition-colors ${
            assigneeFilter() === null
              ? 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900'
              : 'border-slate-200 bg-white text-slate-600 hover:bg-slate-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800'
          }`}
        >
          <span>All</span>
          <span class="opacity-70 tabular-nums">{stats().total}</span>
        </button>

        <For each={stats().options}>
          {(key) => {
            const d = assigneeDisplay(key);
            const active = () => assigneeFilter() === key;
            const count = () => stats().counts.get(key) ?? 0;
            return (
              <button
                type="button"
                onClick={() => setAssigneeFilter(active() ? null : key)}
                aria-pressed={active()}
                class={`inline-flex h-6 max-w-[168px] shrink-0 items-center gap-1 rounded-full border px-2.5 text-[11px] font-medium transition-colors ${
                  active() ? d.solid : d.badge
                }`}
              >
                <span class="shrink-0">{d.icon}</span>
                <span class="truncate">{d.label}</span>
                <Show when={count() > 0}>
                  <span class="opacity-70 tabular-nums">{count()}</span>
                </Show>
              </button>
            );
          }}
        </For>
      </div>
    </Show>
  );
}
