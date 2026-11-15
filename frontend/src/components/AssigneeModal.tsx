import { createMemo, createSignal, onCleanup, onMount, Show } from 'solid-js';
import {
  assigneeModalState,
  board,
  createAssignee,
  deleteAssignee,
  setAssigneeModalState,
  updateAssignee,
} from '../store';
import type { AssigneeKind } from '../types';

export default function AssigneeModal() {
  const modal = () => assigneeModalState();
  const editingMember = createMemo(() => {
    const m = modal();
    if (!m || m.mode !== 'edit') return null;
    return board.assignees.find((a) => a.id === m.assigneeId) ?? null;
  });
  const isEdit = () => modal()?.mode === 'edit';

  const [name, setName] = createSignal(editingMember()?.name ?? '');
  const [kind, setKind] = createSignal<AssigneeKind>(editingMember()?.kind ?? 'ai');
  const [roleDescription, setRoleDescription] = createSignal(editingMember()?.role_description ?? '');
  const [nameError, setNameError] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal(false);

  let nameEl!: HTMLInputElement;

  onMount(() => {
    nameEl.focus();
    if (isEdit()) nameEl.select();
  });

  const close = () => setAssigneeModalState(null);

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && !e.defaultPrevented) {
      e.preventDefault();
      e.stopImmediatePropagation();
      close();
    }
  };
  window.addEventListener('keydown', onKey);
  onCleanup(() => window.removeEventListener('keydown', onKey));

  const submit = async (e?: Event) => {
    e?.preventDefault();
    if (saving()) return;

    const cleanedName = name().replace(/\s+/g, ' ').trim();
    if (!cleanedName) {
      setNameError(true);
      nameEl.focus();
      return;
    }

    setSaving(true);
    const m = modal();
    if (m?.mode === 'edit') {
      const updated = await updateAssignee(m.assigneeId, {
        name: cleanedName,
        kind: kind(),
        role_description: roleDescription().trim(),
      });
      setSaving(false);
      if (updated) close();
      return;
    }

    const created = await createAssignee({
      name: cleanedName,
      kind: kind(),
      role_description: roleDescription().trim() ? roleDescription().trim() : null,
      avatar_badge: kind() === 'human' ? '👤' : '🤖',
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
    const ok = await deleteAssignee(m.assigneeId);
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
        aria-labelledby="assignee-modal-title"
        onSubmit={(e) => void submit(e)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void submit();
          }
        }}
        class="modal-in relative w-full max-w-[480px] rounded-lg bg-[#f4f5f7] p-5 shadow-2xl dark:bg-zinc-900 dark:ring-1 dark:ring-zinc-800"
      >
        <div class="flex items-center justify-between pr-8">
          <h2 id="assignee-modal-title" class="text-lg font-semibold text-slate-800 dark:text-zinc-100">
            {isEdit() ? 'Üye / Ajan Düzenle' : 'Üye / AI Ajan Tanımla'}
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

        {/* 1. İsim */}
        <div class="mt-4">
          <label
            for="assignee-name-input"
            class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
          >
            İsim <span class="text-rose-500">*</span>
          </label>
          <input
            id="assignee-name-input"
            ref={nameEl}
            type="text"
            maxLength={80}
            value={name()}
            onInput={(e) => {
              setName(e.currentTarget.value);
              if (nameError()) setNameError(false);
            }}
            placeholder="ör. Yalın, ChatGPT, Claude, Hermes"
            class={`block h-9 w-full rounded-[4px] border bg-white px-3 text-sm text-slate-800 outline-none focus:ring-2 dark:bg-zinc-800 dark:text-zinc-100 ${
              nameError()
                ? 'border-rose-500 focus:border-rose-500 focus:ring-rose-500/30'
                : 'border-slate-300 focus:border-sky-500 focus:ring-sky-500/30 dark:border-zinc-700'
            }`}
          />
          <Show when={nameError()}>
            <p class="mt-1 text-xs text-rose-600 dark:text-rose-400">İsim zorunludur.</p>
          </Show>
        </div>

        {/* 2. Tip Seçimi */}
        <div class="mt-4">
          <div class="mb-1.5 text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500">
            Tip Seçimi
          </div>
          <div
            role="radiogroup"
            aria-label="Üye tipi"
            class="grid grid-cols-2 gap-1.5 rounded-md border border-slate-200 bg-white p-1 dark:border-zinc-700 dark:bg-zinc-800"
          >
            <button
              type="button"
              role="radio"
              aria-checked={kind() === 'human'}
              onClick={() => setKind('human')}
              class={`flex h-9 items-center justify-center gap-1.5 rounded-[4px] text-sm font-medium transition-colors ${
                kind() === 'human'
                  ? 'bg-stone-700 text-white shadow-sm dark:bg-zinc-200 dark:text-zinc-900'
                  : 'text-slate-600 hover:bg-slate-100 dark:text-zinc-300 dark:hover:bg-zinc-700/60'
              }`}
            >
              <span>👤</span>
              <span>İnsan</span>
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={kind() === 'ai'}
              onClick={() => setKind('ai')}
              class={`flex h-9 items-center justify-center gap-1.5 rounded-[4px] text-sm font-medium transition-colors ${
                kind() === 'ai'
                  ? 'bg-indigo-600 text-white shadow-sm dark:bg-indigo-500'
                  : 'text-slate-600 hover:bg-slate-100 dark:text-zinc-300 dark:hover:bg-zinc-700/60'
              }`}
            >
              <span>🤖</span>
              <span>AI Ajan</span>
            </button>
          </div>
        </div>

        {/* 3. Rol & Yetenek Açıklaması */}
        <div class="mt-4">
          <label
            for="assignee-role-input"
            class="mb-1.5 block text-[11px] font-semibold tracking-wide text-slate-500 uppercase dark:text-zinc-500"
          >
            Rol &amp; Yetenek Açıklaması
          </label>
          <textarea
            id="assignee-role-input"
            rows={2}
            maxLength={1000}
            value={roleDescription()}
            onInput={(e) => setRoleDescription(e.currentTarget.value)}
            placeholder='ör. "Frontend & MCP geliştirici" veya "Mimari, araştırma ve spec yazar; kod yazmaz"'
            class="block w-full resize-y rounded-[4px] border border-slate-300 bg-white px-3 py-2 text-sm leading-relaxed text-slate-800 outline-none focus:border-sky-500 focus:ring-2 focus:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
          />
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
                Üyeyi Sil
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
              {isEdit() ? 'Kaydet' : 'Üye / Ajan Ekle'}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
