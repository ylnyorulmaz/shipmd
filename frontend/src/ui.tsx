import { createSignal, For, Show } from 'solid-js';
import { ASSIGNEE_PRESETS, type Assignee, type BuiltinAssignee, type Tag } from './types';

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

interface AssigneeStyle {
  icon: string;
  label: string;
  badge: string;
  solid: string;
}

const BUILTIN_ASSIGNEE_STYLE: Record<BuiltinAssignee, AssigneeStyle> = {
  human: {
    icon: '👤',
    label: 'Me',
    badge:
      'border-slate-200 bg-white text-slate-600 hover:bg-slate-100 dark:border-zinc-600 dark:bg-zinc-700/40 dark:text-zinc-300 dark:hover:bg-zinc-700',
    solid: 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900',
  },
  antigravity: {
    icon: '🚀',
    label: 'Antigravity',
    badge:
      'border-indigo-200 bg-indigo-50 text-indigo-700 hover:bg-indigo-100 dark:border-indigo-400/25 dark:bg-indigo-500/10 dark:text-indigo-300 dark:hover:bg-indigo-500/20',
    solid: 'border-indigo-600 bg-indigo-600 text-white dark:border-indigo-500 dark:bg-indigo-500',
  },
  hermes: {
    icon: '⚡',
    label: 'Hermes',
    badge:
      'border-teal-200 bg-teal-50 text-teal-700 hover:bg-teal-100 dark:border-teal-400/25 dark:bg-teal-500/10 dark:text-teal-300 dark:hover:bg-teal-500/20',
    solid: 'border-teal-600 bg-teal-600 text-white dark:border-teal-500 dark:bg-teal-500',
  },
  agent: {
    icon: '🤖',
    label: 'Agent',
    badge:
      'border-violet-200 bg-violet-50 text-violet-700 hover:bg-violet-100 dark:border-violet-400/25 dark:bg-violet-500/10 dark:text-violet-300 dark:hover:bg-violet-500/20',
    solid: 'border-violet-600 bg-violet-600 text-white dark:border-violet-500 dark:bg-violet-500',
  },
};

const CUSTOM_PALETTE: { badge: string; solid: string }[] = [
  {
    badge:
      'border-rose-200 bg-rose-50 text-rose-700 hover:bg-rose-100 dark:border-rose-400/25 dark:bg-rose-500/10 dark:text-rose-300 dark:hover:bg-rose-500/20',
    solid: 'border-rose-600 bg-rose-600 text-white dark:border-rose-500 dark:bg-rose-500',
  },
  {
    badge:
      'border-amber-200 bg-amber-50 text-amber-800 hover:bg-amber-100 dark:border-amber-400/25 dark:bg-amber-500/10 dark:text-amber-300 dark:hover:bg-amber-500/20',
    solid: 'border-amber-600 bg-amber-600 text-white dark:border-amber-500 dark:bg-amber-500',
  },
  {
    badge:
      'border-cyan-200 bg-cyan-50 text-cyan-800 hover:bg-cyan-100 dark:border-cyan-400/25 dark:bg-cyan-500/10 dark:text-cyan-300 dark:hover:bg-cyan-500/20',
    solid: 'border-cyan-600 bg-cyan-600 text-white dark:border-cyan-500 dark:bg-cyan-500',
  },
  {
    badge:
      'border-fuchsia-200 bg-fuchsia-50 text-fuchsia-700 hover:bg-fuchsia-100 dark:border-fuchsia-400/25 dark:bg-fuchsia-500/10 dark:text-fuchsia-300 dark:hover:bg-fuchsia-500/20',
    solid: 'border-fuchsia-600 bg-fuchsia-600 text-white dark:border-fuchsia-500 dark:bg-fuchsia-500',
  },
];

export function normalizeAssignee(raw: string): string {
  return raw.replace(/[\u0000-\u001f\u007f]/g, '').trim().toLowerCase().slice(0, 64);
}

export function isBuiltinAssignee(a: string): a is BuiltinAssignee {
  return (ASSIGNEE_PRESETS as readonly string[]).includes(a);
}

function customPaletteFor(name: string) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return CUSTOM_PALETTE[hash % CUSTOM_PALETTE.length];
}

export function assigneeDisplay(raw: Assignee) {
  const key = normalizeAssignee(raw) || 'human';
  if (isBuiltinAssignee(key)) {
    return { key, ...BUILTIN_ASSIGNEE_STYLE[key], isCustom: false };
  }
  const palette = customPaletteFor(key);
  return {
    key,
    icon: '✦',
    label: key,
    badge: palette.badge,
    solid: palette.solid,
    isCustom: true,
  };
}

export const assigneeLabel = (a: Assignee) => {
  const d = assigneeDisplay(a);
  return `${d.icon} ${d.label}`;
};

export function AssigneeBadge(props: { assignee: Assignee; onToggle?: () => void; size?: 'sm' | 'md' }) {
  const d = () => assigneeDisplay(props.assignee);
  return (
    <button
      type="button"
      title={`Assigned to ${d().label}${props.onToggle ? ' — click to cycle assignee' : ''}`}
      onClick={(e) => {
        if (!props.onToggle) return;
        e.stopPropagation();
        props.onToggle();
      }}
      onMouseDown={(e) => e.stopPropagation()}
      draggable={false}
      class={`inline-flex max-w-[168px] cursor-pointer select-none items-center gap-1 rounded-full border font-medium transition-colors ${d().badge}`}
      classList={{
        'h-[22px] px-2 text-[11px]': props.size !== 'md',
        'h-7 px-3 text-xs': props.size === 'md',
      }}
    >
      <span class="shrink-0">{d().icon}</span>
      <span class="truncate">{d().label}</span>
    </button>
  );
}

export function AssigneePicker(props: {
  value: Assignee;
  onChange: (next: Assignee) => void;
  size?: 'sm' | 'md';
}) {
  const [editingCustom, setEditingCustom] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  let customInput: HTMLInputElement | undefined;

  const currentKey = () => normalizeAssignee(props.value) || 'human';
  const isCustomActive = () => !isBuiltinAssignee(currentKey());
  const compact = () => props.size !== 'md';

  const openCustom = () => {
    setDraft(isCustomActive() ? currentKey() : '');
    setEditingCustom(true);
    queueMicrotask(() => {
      customInput?.focus();
      customInput?.select();
    });
  };

  const commitCustom = () => {
    if (!editingCustom()) return;
    const cleaned = normalizeAssignee(draft());
    setEditingCustom(false);
    if (cleaned && cleaned !== currentKey()) {
      props.onChange(cleaned);
    }
  };

  const pillSize = () => (compact() ? 'h-6 px-2 text-[11px]' : 'h-7 px-2.5 text-xs');

  return (
    <div class="flex flex-wrap items-center gap-1.5">
      <For each={ASSIGNEE_PRESETS}>
        {(preset) => {
          const d = BUILTIN_ASSIGNEE_STYLE[preset];
          const active = () => currentKey() === preset && !editingCustom();
          return (
            <button
              type="button"
              onClick={() => {
                setEditingCustom(false);
                props.onChange(preset);
              }}
              aria-pressed={active()}
              class={`inline-flex items-center gap-1 rounded-full border font-medium transition-colors ${pillSize()} ${
                active()
                  ? d.solid
                  : 'border-transparent bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10'
              }`}
            >
              <span>{d.icon}</span>
              <span>{d.label}</span>
            </button>
          );
        }}
      </For>

      <Show when={!editingCustom() && isCustomActive() ? assigneeDisplay(currentKey()) : null}>
        {(d) => (
          <button
            type="button"
            onClick={openCustom}
            title="Click to edit custom assignee"
            aria-pressed={true}
            class={`inline-flex max-w-[168px] items-center gap-1 rounded-full border font-medium transition-colors ${pillSize()} ${d().solid}`}
          >
            <span class="shrink-0">{d().icon}</span>
            <span class="truncate">{d().label}</span>
          </button>
        )}
      </Show>

      <Show
        when={editingCustom()}
        fallback={
          <button
            type="button"
            onClick={openCustom}
            class={`inline-flex items-center gap-1 rounded-full border border-dashed border-slate-300 font-medium text-slate-500 transition-colors hover:border-slate-400 hover:text-slate-700 dark:border-zinc-700 dark:text-zinc-400 dark:hover:border-zinc-500 dark:hover:text-zinc-200 ${pillSize()}`}
          >
            {isCustomActive() ? '✎ Edit' : '+ Custom…'}
          </button>
        }
      >
        <div class="inline-flex items-center gap-1">
          <input
            ref={customInput}
            type="text"
            maxLength={64}
            value={draft()}
            placeholder="name (e.g. codex)"
            aria-label="Custom assignee name"
            onInput={(e) => setDraft(e.currentTarget.value)}
            onBlur={commitCustom}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                e.stopPropagation();
                commitCustom();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                e.stopPropagation();
                setEditingCustom(false);
              }
            }}
            class={`w-32 rounded-full border border-sky-400 bg-white px-2.5 text-slate-800 outline-none focus:ring-2 focus:ring-sky-500/30 dark:border-sky-500 dark:bg-zinc-800 dark:text-zinc-100 ${
              compact() ? 'h-6 text-[11px]' : 'h-7 text-xs'
            }`}
          />
          <button
            type="button"
            aria-label="Apply custom assignee"
            onMouseDown={(e) => e.preventDefault()}
            onClick={commitCustom}
            class={`rounded-full bg-sky-600 font-medium text-white hover:bg-sky-700 ${
              compact() ? 'h-6 px-2 text-[11px]' : 'h-7 px-2.5 text-xs'
            }`}
          >
            Set
          </button>
        </div>
      </Show>
    </div>
  );
}
