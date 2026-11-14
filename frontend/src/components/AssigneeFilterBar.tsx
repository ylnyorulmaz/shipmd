import { createMemo, For, Show } from 'solid-js';
import {
  assigneeFilter,
  board,
  clearAllFilters,
  hasActiveFilters,
  labelFilter,
  repoFilter,
  setAssigneeFilter,
  setLabelFilter,
  setRepoFilter,
} from '../store';
import { ASSIGNEE_PRESETS, REPO_PRESETS } from '../types';
import {
  assigneeDisplay,
  isBuiltinAssignee,
  labelStyleFor,
  normalizeAssignee,
  normalizeLabels,
  normalizeRepo,
} from '../ui';

export default function AssigneeFilterBar() {
  const stats = createMemo(() => {
    const assigneeCounts = new Map<string, number>();
    const repoCounts = new Map<string, number>();
    const labelCounts = new Map<string, number>();
    let total = 0;

    for (const c of Object.values(board.cards)) {
      total++;
      const aKey = normalizeAssignee(c.assignee) || 'human';
      assigneeCounts.set(aKey, (assigneeCounts.get(aKey) ?? 0) + 1);

      if (c.repo) {
        const rKey = normalizeRepo(c.repo);
        if (rKey) repoCounts.set(rKey, (repoCounts.get(rKey) ?? 0) + 1);
      }

      for (const lbl of normalizeLabels(c.labels)) {
        labelCounts.set(lbl, (labelCounts.get(lbl) ?? 0) + 1);
      }
    }

    const customAssignees = new Set<string>();
    for (const k of assigneeCounts.keys()) {
      if (!isBuiltinAssignee(k)) customAssignees.add(k);
    }
    const activeA = assigneeFilter();
    if (activeA && !isBuiltinAssignee(activeA)) customAssignees.add(activeA);
    const assigneeOptions: string[] = [...ASSIGNEE_PRESETS, ...[...customAssignees].sort()];

    const customRepos = new Set<string>();
    const isPresetRepo = (r: string) => (REPO_PRESETS as readonly string[]).includes(r);
    for (const r of repoCounts.keys()) {
      if (!isPresetRepo(r)) customRepos.add(r);
    }
    const activeR = repoFilter();
    if (activeR && !isPresetRepo(activeR)) customRepos.add(activeR);
    const repoOptions: string[] = [...REPO_PRESETS, ...[...customRepos].sort()];

    const labelSet = new Set<string>(labelCounts.keys());
    const activeL = labelFilter();
    if (activeL) labelSet.add(activeL);
    const labelOptions = [...labelSet].sort();

    return {
      assigneeCounts,
      repoCounts,
      labelCounts,
      total,
      assigneeOptions,
      repoOptions,
      labelOptions,
    };
  });

  return (
    <Show when={board.loaded}>
      <div class="shrink-0 border-b border-slate-200/80 bg-white/75 px-4 dark:border-zinc-800/80 dark:bg-zinc-950/75">
        {/* Assignee row */}
        <div
          aria-label="Filter by assignee"
          class="flex items-center gap-1.5 overflow-x-auto py-1.5 [scrollbar-width:none]"
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

          <For each={stats().assigneeOptions}>
            {(key) => {
              const d = assigneeDisplay(key);
              const active = () => assigneeFilter() === key;
              const count = () => stats().assigneeCounts.get(key) ?? 0;
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

          <Show when={hasActiveFilters()}>
            <button
              type="button"
              onClick={clearAllFilters}
              class="ml-auto inline-flex h-6 shrink-0 items-center gap-1 rounded-full border border-slate-200 bg-slate-100 px-2.5 text-[11px] font-medium text-slate-600 hover:bg-slate-200 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
            >
              <span>× Clear filters</span>
            </button>
          </Show>
        </div>

        {/* Repo & Label row */}
        <div
          aria-label="Filter by repo and labels"
          class="flex items-center gap-1.5 overflow-x-auto border-t border-slate-200/60 py-1 [scrollbar-width:none] dark:border-zinc-800/60"
        >
          <span class="mr-1 shrink-0 text-[11px] font-semibold tracking-wide text-slate-400 uppercase dark:text-zinc-500">
            Repo
          </span>
          <button
            type="button"
            onClick={() => setRepoFilter(null)}
            aria-pressed={repoFilter() === null}
            class={`inline-flex h-5 shrink-0 items-center rounded-full border px-2 text-[10.5px] font-medium transition-colors ${
              repoFilter() === null
                ? 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900'
                : 'border-slate-200 bg-white text-slate-600 hover:bg-slate-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800'
            }`}
          >
            All
          </button>
          <For each={stats().repoOptions}>
            {(repo) => {
              const active = () => repoFilter() === repo;
              const count = () => stats().repoCounts.get(repo) ?? 0;
              return (
                <button
                  type="button"
                  onClick={() => setRepoFilter(active() ? null : repo)}
                  aria-pressed={active()}
                  class={`inline-flex h-5 max-w-[140px] shrink-0 items-center gap-1 rounded-full border px-2 font-mono text-[10.5px] font-medium transition-colors ${
                    active()
                      ? 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900'
                      : 'border-slate-200 bg-white text-slate-600 hover:bg-slate-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800'
                  }`}
                >
                  <span class="truncate">{repo}</span>
                  <Show when={count() > 0}>
                    <span class="opacity-70 tabular-nums">{count()}</span>
                  </Show>
                </button>
              );
            }}
          </For>

          <Show when={stats().labelOptions.length > 0}>
            <span class="mx-1.5 h-3.5 w-px shrink-0 bg-slate-200 dark:bg-zinc-800" />
            <span class="mr-1 shrink-0 text-[11px] font-semibold tracking-wide text-slate-400 uppercase dark:text-zinc-500">
              Labels
            </span>
            <For each={stats().labelOptions}>
              {(lbl) => {
                const active = () => labelFilter() === lbl;
                const count = () => stats().labelCounts.get(lbl) ?? 0;
                return (
                  <button
                    type="button"
                    onClick={() => setLabelFilter(active() ? null : lbl)}
                    aria-pressed={active()}
                    class={`inline-flex h-5 max-w-[140px] shrink-0 items-center gap-1 rounded-full border px-2 text-[10.5px] font-medium transition-colors ${
                      active()
                        ? 'border-sky-600 bg-sky-600 text-white dark:border-sky-500 dark:bg-sky-500'
                        : labelStyleFor(lbl)
                    }`}
                  >
                    <span class="truncate">#{lbl}</span>
                    <Show when={count() > 0}>
                      <span class="opacity-70 tabular-nums">{count()}</span>
                    </Show>
                  </button>
                );
              }}
            </For>
          </Show>
        </div>
      </div>
    </Show>
  );
}
