import { createMemo, For, Show } from 'solid-js';
import {
  assigneeFilter,
  board,
  clearAllFilters,
  hasActiveFilters,
  labelFilter,
  repoFilter,
  setAssigneeFilter,
  setAssigneeModalState,
  setLabelFilter,
  setProjectModalState,
  setRepoFilter,
} from '../store';
import {
  assigneeDisplay,
  AssigneeKindChip,
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

    const knownAssigneeIds = new Set(board.assignees.map((m) => m.id));
    const orphanAssignees = new Set<string>();
    for (const k of assigneeCounts.keys()) {
      if (!knownAssigneeIds.has(k)) orphanAssignees.add(k);
    }
    const activeA = assigneeFilter();
    if (activeA && !knownAssigneeIds.has(activeA)) orphanAssignees.add(activeA);
    const assigneeOptions: string[] = [
      ...board.assignees.map((m) => m.id),
      ...[...orphanAssignees].sort(),
    ];

    const knownProjectIds = new Set(board.projects.map((p) => p.id));
    const orphanRepos = new Set<string>();
    for (const r of repoCounts.keys()) {
      if (!knownProjectIds.has(r)) orphanRepos.add(r);
    }
    const activeR = repoFilter();
    if (activeR && !knownProjectIds.has(activeR)) orphanRepos.add(activeR);
    const repoOptions = [
      ...board.projects.map((p) => ({
        id: p.id,
        name: p.name,
        description: p.description,
      })),
      ...[...orphanRepos].sort().map((id) => ({
        id,
        name: id,
        description: null as string | null,
      })),
    ];

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

  const selectedProject = createMemo(() => {
    const active = repoFilter();
    if (!active) return null;
    return board.projects.find((p) => p.id === active) ?? null;
  });

  const selectedAssignee = createMemo(() => {
    const active = assigneeFilter();
    if (!active) return null;
    return board.assignees.find((a) => a.id === active) ?? null;
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
              const d = () => assigneeDisplay(key);
              const active = () => assigneeFilter() === key;
              const count = () => stats().assigneeCounts.get(key) ?? 0;
              const titleText = () => {
                const info = d();
                const kindLabel = info.kind === 'ai' ? 'AI Agent' : 'Human';
                return info.roleDescription
                  ? `${info.label} (${kindLabel}) — ${info.roleDescription}`
                  : `${info.label} (${kindLabel})`;
              };
              return (
                <button
                  type="button"
                  title={titleText()}
                  onClick={() => setAssigneeFilter(active() ? null : key)}
                  aria-pressed={active()}
                  class={`inline-flex h-6 max-w-[184px] shrink-0 items-center gap-1 rounded-full border px-2.5 text-[11px] font-medium transition-colors ${
                    active() ? d().solid : d().badge
                  }`}
                  classList={{ 'border-dashed': !active() && d().kind === 'ai' }}
                >
                  <span class="shrink-0">{d().icon}</span>
                  <span class="truncate">{d().label}</span>
                  <AssigneeKindChip kind={d().kind} active={active()} />
                  <Show when={count() > 0}>
                    <span class="opacity-70 tabular-nums">{count()}</span>
                  </Show>
                </button>
              );
            }}
          </For>

          <div class="ml-auto flex shrink-0 items-center gap-1.5 pl-2">
            <Show when={selectedAssignee()}>
              {(member) => (
                <button
                  type="button"
                  title={`Üye / Ajan düzenle: ${member().name}`}
                  aria-label={`Edit ${member().name}`}
                  onClick={() => setAssigneeModalState({ mode: 'edit', assigneeId: member().id })}
                  class="inline-flex h-6 items-center gap-1 rounded-full border border-slate-200 bg-white px-2 text-[11px] font-medium text-slate-600 transition-colors hover:border-slate-300 hover:bg-slate-100 hover:text-slate-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
                >
                  <span aria-hidden="true">✎</span>
                </button>
              )}
            </Show>
            <button
              type="button"
              onClick={() => setAssigneeModalState({ mode: 'create' })}
              class="inline-flex h-6 shrink-0 items-center gap-1 rounded-full border border-dashed border-slate-300 bg-white px-2.5 text-[11px] font-medium text-slate-600 transition-colors hover:border-sky-400 hover:text-sky-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:border-sky-500 dark:hover:text-sky-300"
            >
              <span>+ Üye / Ajan</span>
            </button>
            <Show when={hasActiveFilters()}>
              <button
                type="button"
                onClick={clearAllFilters}
                class="inline-flex h-6 shrink-0 items-center gap-1 rounded-full border border-slate-200 bg-slate-100 px-2.5 text-[11px] font-medium text-slate-600 hover:bg-slate-200 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300 dark:hover:bg-zinc-700"
              >
                <span>× Clear</span>
              </button>
            </Show>
          </div>
        </div>

        {/* Repo / Project & Label row */}
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
            {(proj) => {
              const active = () => repoFilter() === proj.id;
              const count = () => stats().repoCounts.get(proj.id) ?? 0;
              return (
                <button
                  type="button"
                  title={proj.description ? `${proj.name} — ${proj.description}` : proj.name}
                  onClick={() => setRepoFilter(active() ? null : proj.id)}
                  aria-pressed={active()}
                  class={`inline-flex h-5 max-w-[160px] shrink-0 items-center gap-1 rounded-full border px-2 font-mono text-[10.5px] font-medium transition-colors ${
                    active()
                      ? 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900'
                      : 'border-slate-200 bg-white text-slate-600 hover:bg-slate-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800'
                  }`}
                >
                  <span class="truncate">{proj.name}</span>
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

          <div class="ml-auto flex shrink-0 items-center gap-1.5 pl-2">
            <Show when={selectedProject()}>
              {(proj) => (
                <button
                  type="button"
                  title={`Projeyi düzenle: ${proj().name}`}
                  aria-label={`Edit project ${proj().name}`}
                  onClick={() => setProjectModalState({ mode: 'edit', projectId: proj().id })}
                  class="inline-flex h-5 items-center gap-1 rounded-full border border-slate-200 bg-white px-2 text-[10.5px] font-medium text-slate-600 transition-colors hover:border-slate-300 hover:bg-slate-100 hover:text-slate-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
                >
                  <span aria-hidden="true">✎</span>
                </button>
              )}
            </Show>
            <button
              type="button"
              onClick={() => setProjectModalState({ mode: 'create' })}
              class="inline-flex h-5 shrink-0 items-center gap-1 rounded-full border border-dashed border-slate-300 bg-white px-2 text-[10.5px] font-medium text-slate-600 transition-colors hover:border-sky-400 hover:text-sky-700 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:border-sky-500 dark:hover:text-sky-300"
            >
              <span>+ Proje</span>
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
