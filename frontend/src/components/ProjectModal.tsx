import { createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import {
  board,
  createProject,
  deleteProject,
  projectModalState,
  setProjectModalState,
  updateProject,
} from '../store';
import type { ProjectLink } from '../types';

export default function ProjectModal() {
  const modal = () => projectModalState();
  const editingProject = createMemo(() => {
    const m = modal();
    if (!m || m.mode !== 'edit') return null;
    return board.projects.find((p) => p.id === m.projectId) ?? null;
  });
  const isEdit = () => modal()?.mode === 'edit';

  const [name, setName] = createSignal(editingProject()?.name ?? '');
  const [description, setDescription] = createSignal(editingProject()?.description ?? '');
  const [links, setLinks] = createSignal<ProjectLink[]>(
    editingProject()?.links.length ? editingProject()!.links.map((l) => ({ ...l })) : [{ label: '', url: '' }],
  );
  const [nameError, setNameError] = createSignal(false);
  const [linkError, setLinkError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal(false);

  let nameEl!: HTMLInputElement;

  onMount(() => {
    nameEl.focus();
    if (isEdit()) nameEl.select();
  });

  const close = () => setProjectModalState(null);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && !e.defaultPrevented) {
      e.preventDefault();
      e.stopImmediatePropagation();
      close();
    }
  };
  window.addEventListener('keydown', onKey);
  onCleanup(() => window.removeEventListener('keydown', onKey));

  const addLinkRow = () => {
    setLinks((prev) => [...prev, { label: '', url: '' }]);
  };

  const updateLinkRow = (idx: number, field: 'label' | 'url', value: string) => {
    setLinkError(null);
    setLinks((prev) => prev.map((row, i) => (i === idx ? { ...row, [field]: value } : row)));
  };

  const removeLinkRow = (idx: number) => {
    setLinkError(null);
    setLinks((prev) => prev.filter((_, i) => i !== idx));
  };

  const submit = async (e?: Event) => {
    e?.preventDefault();
    if (saving()) return;

    const cleanedName = name().replace(/\s+/g, ' ').trim();
    if (!cleanedName) {
      setNameError(true);
      nameEl.focus();
      return;
    }

    const cleanedLinks: ProjectLink[] = [];
    for (const row of links()) {
      const label = row.label.trim();
      const url = row.url.trim();
      if (!label && !url) continue;
      if (!label || !url) {
        setLinkError('Her link satırı için hem etiket hem URL girin.');
        return;
      }
      cleanedLinks.push({ label, url });
    }

    setSaving(true);
    const m = modal();
    if (m?.mode === 'edit') {
      const updated = await updateProject(m.projectId, {
        name: cleanedName,
        description: description().trim(),
        links: cleanedLinks,
      });
      setSaving(false);
      if (updated) close();
      return;
    }

    const created = await createProject({
      name: cleanedName,
      description: description().trim() ? description().trim() : null,
      links: cleanedLinks,
    });
    setSaving(false);
    if (created) {
      if (m?.mode === 'create') m.onCreated?.(created);
      close();
    }
  };

  const handleDelete = async () => {
    const m = modal();
    if (!m || m.mode !== 'edit' || saving()) return;
    setSaving(true);
    const ok = await deleteProject(m.projectId);
    setSaving(false);
    if (ok) close();
  };

  return (
    <div
      class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-slate-900/50 px-4 py-12 backdrop-blur-[1px] dark:bg-black/60"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-labelledby="project-modal-title"
        onSubmit={(e) => void submit(e)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void submit();
          }
        }}
        class="modal-in relative w-full max-w-[540px] rounded-lg bg-[#f4f5f7] p-5 shadow-2xl dark:bg-zinc-900 dark:ring-1 dark:ring-zinc-800"
      >
        <div class="flex items-center justify-between pr-8">
          <h2 id="project-modal-title" class="text-lg font-semibold text-slate-800 dark:text-zinc-100">
            {isEdit() ? 'Projeyi Düzenle' : 'Proje / Workspace Tanımla'}
          </h2>
          <span class="text-[11px] text-slate-400 dark:text-zinc-500">⌘/Ctrl + ↵</span>
        </div>

        <button
          type="button"
          aria-label="Close"
          onClick={close}
          class="absolute top-3 right-3 grid h-8 w-8 place-items-center rounded-md text-xl text-slate-500 hover:bg-black/5 hover:text-slate-800 dark:text-zinc-400 dark:hover:bg-white/10 dark:hover:text-zinc-100"
        >
          ×
        </button>

        {/* 1. Proje Adı */}
        <div class="mt-4">
          <label
            for="project-name-input"
            class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
          >
            Proje Adı <span class="text-rose-500">*</span>
          </label>
          <input
            id="project-name-input"
            ref={nameEl}
            type="text"
            maxLength={80}
            value={name()}
            onInput={(e) => {
              setName(e.currentTarget.value);
              if (nameError()) setNameError(false);
            }}
            placeholder="ör. ShipMD, CodeSight, Q4 Marketing"
            class={`block h-9 w-full rounded-[4px] border bg-white px-3 text-sm text-slate-800 outline-none focus:ring-2 dark:bg-zinc-800 dark:text-zinc-100 ${
              nameError()
                ? 'border-rose-500 focus:border-rose-500 focus:ring-rose-500/30'
                : 'border-slate-300 focus:border-sky-500 focus:ring-sky-500/30 dark:border-zinc-700'
            }`}
          />
          <Show when={nameError()}>
            <p class="mt-1 text-xs text-rose-600 dark:text-rose-400">Proje adı zorunludur.</p>
          </Show>
        </div>

        {/* 2. Kısa Açıklama / Bağlam */}
        <div class="mt-4">
          <label
            for="project-desc-input"
            class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
          >
            Kısa Açıklama / Bağlam
          </label>
          <textarea
            id="project-desc-input"
            rows={3}
            maxLength={1000}
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="Projenin amacı, hedef kitlesi veya ajanların bilmesi gereken kısa bağlam…"
            class="block w-full resize-y rounded-[4px] border border-slate-300 bg-white px-3 py-2 text-sm leading-relaxed text-slate-800 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
          />
        </div>

        {/* 3. Kaynak Linkleri */}
        <div class="mt-4">
          <div class="mb-1.5 flex items-center justify-between">
            <span class="text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">
              Kaynak Linkleri
            </span>
            <button
              type="button"
              onClick={addLinkRow}
              class="inline-flex h-6 items-center gap-1 rounded-[4px] border border-slate-300 bg-white px-2 text-[11px] font-medium text-slate-700 hover:bg-slate-50 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-200 dark:hover:bg-zinc-700"
            >
              + Link Ekle
            </button>
          </div>

          <div class="space-y-2">
            <For each={links()}>
              {(row, idx) => (
                <div class="flex items-center gap-1.5">
                  <input
                    type="text"
                    maxLength={40}
                    value={row.label}
                    aria-label="Link etiketi"
                    placeholder="Etiket (GitHub, Live, CRM…)"
                    onInput={(e) => updateLinkRow(idx(), 'label', e.currentTarget.value)}
                    class="h-8 w-36 shrink-0 rounded-[4px] border border-slate-300 bg-white px-2.5 text-xs text-slate-800 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                  />
                  <input
                    type="url"
                    maxLength={2048}
                    value={row.url}
                    aria-label="Link URL"
                    placeholder="https://..."
                    onInput={(e) => updateLinkRow(idx(), 'url', e.currentTarget.value)}
                    class="h-8 min-w-0 flex-1 rounded-[4px] border border-slate-300 bg-white px-2.5 font-mono text-xs text-slate-800 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
                  />
                  <button
                    type="button"
                    aria-label="Linki kaldır"
                    onClick={() => removeLinkRow(idx())}
                    class="grid h-8 w-8 shrink-0 place-items-center rounded-[4px] text-slate-400 hover:bg-black/5 hover:text-rose-600 dark:text-zinc-500 dark:hover:bg-white/10 dark:hover:text-rose-400"
                  >
                    ×
                  </button>
                </div>
              )}
            </For>
            <Show when={links().length === 0}>
              <p class="rounded-[4px] border border-dashed border-slate-300 px-3 py-2 text-xs text-slate-400 dark:border-zinc-700 dark:text-zinc-500">
                Henüz kaynak linki yok — eklemek için "+ Link Ekle" kullanın.
              </p>
            </Show>
          </div>
          <Show when={linkError()}>
            {(err) => <p class="mt-1 text-xs text-rose-600 dark:text-rose-400">{err()}</p>}
          </Show>
        </div>

        {/* Actions */}
        <div class="mt-6 flex flex-wrap items-center gap-2 border-t border-slate-200 pt-4 dark:border-zinc-800">
          <Show when={isEdit()}>
            <Show
              when={confirmDelete()}
              fallback={
                <button
                  type="button"
                  onClick={() => setConfirmDelete(true)}
                  class="h-9 rounded-[4px] px-3 text-sm text-slate-500 hover:bg-red-50 hover:text-red-600 dark:text-zinc-400 dark:hover:bg-red-500/10 dark:hover:text-red-400"
                >
                  Sil
                </button>
              }
            >
              <button
                type="button"
                onClick={() => setConfirmDelete(false)}
                class="h-9 rounded-[4px] px-2.5 text-sm text-slate-500 hover:bg-black/5 dark:text-zinc-400 dark:hover:bg-white/10"
              >
                Vazgeç
              </button>
              <button
                type="button"
                disabled={saving()}
                onClick={() => void handleDelete()}
                class="h-9 rounded-[4px] bg-red-600 px-3 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                Projeyi Sil
              </button>
            </Show>
          </Show>

          <div class="ml-auto flex items-center gap-2">
            <button
              type="button"
              onClick={close}
              class="h-9 rounded-[4px] px-3.5 text-sm font-medium text-slate-600 hover:bg-black/5 dark:text-zinc-400 dark:hover:bg-white/10"
            >
              İptal
            </button>
            <button
              type="submit"
              disabled={saving()}
              class="h-9 rounded-[4px] bg-sky-600 px-4 text-sm font-medium text-white shadow-sm hover:bg-sky-700 active:bg-sky-800 disabled:opacity-50"
            >
              {isEdit() ? 'Kaydet' : 'Proje Oluştur'}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
