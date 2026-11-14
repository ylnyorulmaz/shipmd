import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { board, createCard, setCreateModalOpen } from '../store';
import { TAGS, type Assignee, type Tag } from '../types';
import { AssigneePicker, LabelsInput, PriorityPicker, RepoPicker, TAG_LABEL, TAG_SOLID } from '../ui';

export default function CreateCardModal() {
  const defaultColumn = () => board.columns[0]?.id ?? 'backlog';

  const [title, setTitle] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [columnId, setColumnId] = createSignal(defaultColumn());
  const [tag, setTag] = createSignal<Tag>('build');
  const [assignee, setAssignee] = createSignal<Assignee>('human');
  const [repo, setRepo] = createSignal<string | null>(null);
  const [priority, setPriority] = createSignal<number>(0);
  const [labels, setLabels] = createSignal<string[]>([]);
  const [titleError, setTitleError] = createSignal(false);

  let titleEl!: HTMLInputElement;

  onMount(() => {
    titleEl.focus();
  });

  const close = () => setCreateModalOpen(false);

  // Bubble-phase Escape allows inline custom inputs (Assignee/Repo) to close first.
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && !e.defaultPrevented) {
      e.preventDefault();
      close();
    }
  };
  window.addEventListener('keydown', onKey);
  onCleanup(() => window.removeEventListener('keydown', onKey));

  const submit = (e?: Event) => {
    e?.preventDefault();
    const cleanedTitle = title().replace(/\s+/g, ' ').trim();
    if (!cleanedTitle) {
      setTitleError(true);
      titleEl.focus();
      return;
    }
    void createCard({
      column_id: columnId(),
      title: cleanedTitle,
      description: description().trim() ? description() : null,
      tag: tag(),
      assignee: assignee(),
      repo: repo(),
      priority: priority(),
      labels: labels(),
    });
    close();
  };

  return (
    <div
      class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-slate-900/50 px-4 py-10 backdrop-blur-[1px] dark:bg-black/60"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-card-title"
        onSubmit={submit}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            submit();
          }
        }}
        class="modal-in relative w-full max-w-[640px] rounded-lg bg-[#f4f5f7] p-5 shadow-2xl dark:bg-zinc-900 dark:ring-1 dark:ring-zinc-800"
      >
        <div class="flex items-center justify-between pr-8">
          <h2 id="create-card-title" class="text-lg font-semibold text-slate-800 dark:text-zinc-100">
            Create Card
          </h2>
          <span class="text-[11px] text-slate-400 dark:text-zinc-500">⌘/Ctrl + ↵ to create</span>
        </div>

        <button
          type="button"
          aria-label="Close"
          onClick={close}
          class="absolute top-3 right-3 grid h-8 w-8 place-items-center rounded-md text-xl text-slate-500 hover:bg-black/5 hover:text-slate-800 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-zinc-100"
        >
          ×
        </button>

        {/* Title */}
        <div class="mt-4">
          <label
            for="new-card-title"
            class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
          >
            Title <span class="text-rose-500">*</span>
          </label>
          <input
            id="new-card-title"
            ref={titleEl}
            type="text"
            maxLength={500}
            value={title()}
            onInput={(e) => {
              setTitle(e.currentTarget.value);
              if (titleError()) setTitleError(false);
            }}
            placeholder="Short, imperative task title…"
            class={`block h-9 w-full rounded-[4px] border bg-white px-3 text-sm text-slate-800 outline-none focus:ring-2 dark:bg-zinc-800 dark:text-zinc-100 ${
              titleError()
                ? 'border-rose-500 focus:border-rose-500 focus:ring-rose-500/30'
                : 'border-slate-300 focus:border-sky-500 focus:ring-sky-500/30 dark:border-zinc-700'
            }`}
          />
          <Show when={titleError()}>
            <p class="mt-1 text-xs text-rose-600 dark:text-rose-400">Title is required.</p>
          </Show>
        </div>

        {/* Description */}
        <div class="mt-4">
          <label
            for="new-card-desc"
            class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
          >
            Description <span class="font-normal normal-case opacity-70">(Markdown supported)</span>
          </label>
          <textarea
            id="new-card-desc"
            rows={4}
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="Acceptance criteria, links, - [ ] checklists…"
            class="block w-full resize-y rounded-[4px] border border-slate-300 bg-white px-3 py-2 font-mono text-[13px] leading-relaxed text-slate-800 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
          />
        </div>

        {/* Column & Tag */}
        <div class="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div>
            <label
              for="new-card-col"
              class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
            >
              Column / List
            </label>
            <select
              id="new-card-col"
              value={columnId()}
              onChange={(e) => setColumnId(e.currentTarget.value)}
              class="h-8 w-full rounded-[4px] border border-slate-300 bg-white px-2 text-sm text-slate-700 outline-none focus:ring-2 focus:ring-sky-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200"
            >
              <For each={board.columns}>{(c) => <option value={c.id}>{c.title}</option>}</For>
            </select>
          </div>

          <div>
            <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">Tag</div>
            <div class="flex gap-1.5">
              <For each={TAGS}>
                {(t) => (
                  <button
                    type="button"
                    onClick={() => setTag(t)}
                    class="h-8 rounded-[3px] px-3 text-[11px] font-semibold tracking-wide uppercase transition-colors"
                    classList={{
                      [TAG_SOLID[t]]: tag() === t,
                      'bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10':
                        tag() !== t,
                    }}
                  >
                    {TAG_LABEL[t]}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        {/* Assignee */}
        <div class="mt-4">
          <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">Assignee</div>
          <AssigneePicker size="md" value={assignee()} onChange={setAssignee} />
        </div>

        {/* Repo / Project */}
        <div class="mt-4">
          <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">
            Repo / Project
          </div>
          <RepoPicker size="md" value={repo()} onChange={setRepo} />
        </div>

        {/* Priority */}
        <div class="mt-4">
          <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">Priority</div>
          <PriorityPicker size="md" value={priority()} onChange={setPriority} />
        </div>

        {/* Labels */}
        <div class="mt-4">
          <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">
            Labels <span class="font-normal normal-case opacity-70">(#etiket, Enter or comma)</span>
          </div>
          <LabelsInput value={labels()} onChange={setLabels} />
        </div>

        {/* Actions */}
        <div class="mt-6 flex items-center justify-end gap-2 border-t border-slate-200 pt-4 dark:border-zinc-800">
          <button
            type="button"
            onClick={close}
            class="h-9 rounded-[4px] px-3.5 text-sm font-medium text-slate-600 hover:bg-black/5 dark:text-zinc-400 dark:hover:bg-white/10"
          >
            Cancel
          </button>
          <button
            type="submit"
            class="h-9 rounded-[4px] bg-sky-600 px-4 text-sm font-medium text-white shadow-sm hover:bg-sky-700 active:bg-sky-800"
          >
            Create Card
          </button>
        </div>
      </form>
    </div>
  );
}
