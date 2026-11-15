import { createMemo, createSignal, For, Show } from 'solid-js';
import { board, setAssigneeModalState, setProjectModalState } from './store';
import {
  ASSIGNEE_PRESETS,
  PRIORITY_OPTIONS,
  type Assignee,
  type AssigneeKind,
  type AssigneeMember,
  type BuiltinAssignee,
  type Tag,
} from './types';

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
  kimi: {
    icon: '🌙',
    label: 'Kimi',
    badge:
      'border-orange-200 bg-orange-50 text-orange-700 hover:bg-orange-100 dark:border-orange-400/25 dark:bg-orange-500/10 dark:text-orange-300 dark:hover:bg-orange-500/20',
    solid: 'border-orange-600 bg-orange-600 text-white dark:border-orange-500 dark:bg-orange-500',
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

const HUMAN_CUSTOM_PALETTE: { badge: string; solid: string } = {
  badge:
    'border-stone-300 bg-stone-50 text-stone-700 hover:bg-stone-100 dark:border-zinc-600 dark:bg-zinc-700/50 dark:text-zinc-200 dark:hover:bg-zinc-700',
  solid: 'border-stone-700 bg-stone-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900',
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

export interface AssigneeDisplay {
  key: string;
  icon: string;
  label: string;
  kind: AssigneeKind;
  roleDescription: string | null;
  badge: string;
  solid: string;
  isCustom: boolean;
}

export function assigneeDisplay(raw: Assignee, memberOverride?: AssigneeMember): AssigneeDisplay {
  const key = normalizeAssignee(raw) || 'human';
  const member = memberOverride ?? board.assignees.find((m) => m.id === key);

  if (member) {
    const kind: AssigneeKind = member.kind === 'human' ? 'human' : 'ai';
    const builtin = isBuiltinAssignee(key) ? BUILTIN_ASSIGNEE_STYLE[key] : null;
    const palette = builtin
      ? { badge: builtin.badge, solid: builtin.solid }
      : kind === 'human'
        ? HUMAN_CUSTOM_PALETTE
        : customPaletteFor(key);
    return {
      key,
      icon: member.avatar_badge || builtin?.icon || (kind === 'human' ? '👤' : '🤖'),
      label: member.name || builtin?.label || key,
      kind,
      roleDescription: member.role_description ?? null,
      badge: palette.badge,
      solid: palette.solid,
      isCustom: false,
    };
  }

  if (isBuiltinAssignee(key)) {
    return {
      key,
      ...BUILTIN_ASSIGNEE_STYLE[key],
      kind: key === 'human' ? 'human' : 'ai',
      roleDescription: null,
      isCustom: false,
    };
  }
  const palette = customPaletteFor(key);
  return {
    key,
    icon: '✦',
    label: key,
    kind: 'ai',
    roleDescription: null,
    badge: palette.badge,
    solid: palette.solid,
    isCustom: true,
  };
}

export const assigneeLabel = (a: Assignee) => {
  const d = assigneeDisplay(a);
  return `${d.icon} ${d.label}`;
};

export function AssigneeKindChip(props: { kind: AssigneeKind; active?: boolean }) {
  return (
    <Show when={props.kind === 'ai'}>
      <span
        class={`shrink-0 rounded px-1 py-px text-[8.5px] leading-none font-bold uppercase tracking-wider ${
          props.active
            ? 'bg-white/25 text-white'
            : 'bg-black/8 text-current opacity-75 dark:bg-white/12'
        }`}
      >
        AI
      </span>
    </Show>
  );
}

export function AssigneeBadge(props: { assignee: Assignee; onToggle?: () => void; size?: 'sm' | 'md' }) {
  const d = () => assigneeDisplay(props.assignee);
  const titleText = () => {
    const base = `${d().label} (${d().kind === 'ai' ? 'AI Agent' : 'Human'})`;
    const withRole = d().roleDescription ? `${base} — ${d().roleDescription}` : base;
    return props.onToggle ? `${withRole} · click to cycle assignee` : withRole;
  };
  return (
    <button
      type="button"
      title={titleText()}
      onClick={(e) => {
        if (!props.onToggle) return;
        e.stopPropagation();
        props.onToggle();
      }}
      onMouseDown={(e) => e.stopPropagation()}
      draggable={false}
      class={`inline-flex max-w-[184px] cursor-pointer select-none items-center gap-1 rounded-full border font-medium transition-colors ${d().badge}`}
      classList={{
        'h-[22px] px-2 text-[11px]': props.size !== 'md',
        'h-7 px-3 text-xs': props.size === 'md',
        'border-dashed': d().kind === 'ai',
      }}
    >
      <span class="shrink-0">{d().icon}</span>
      <span class="truncate">{d().label}</span>
      <AssigneeKindChip kind={d().kind} />
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
  const knownAssignees = createMemo(() => {
    if (board.assignees.length > 0) return board.assignees;
    return ASSIGNEE_PRESETS.map((id) => ({
      id,
      name: BUILTIN_ASSIGNEE_STYLE[id].label,
      kind: (id === 'human' ? 'human' : 'ai') as AssigneeKind,
      role_description: null,
      avatar_badge: BUILTIN_ASSIGNEE_STYLE[id].icon,
      updated_at: '',
    }));
  });
  const isKnown = (k: string) => knownAssignees().some((m) => m.id === k);
  const isCustomActive = () => !isKnown(currentKey());
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
      <For each={knownAssignees()}>
        {(member) => {
          const d = () => assigneeDisplay(member.id, member);
          const active = () => currentKey() === member.id && !editingCustom();
          return (
            <button
              type="button"
              title={member.role_description ? `${member.name} — ${member.role_description}` : member.name}
              onClick={() => {
                setEditingCustom(false);
                props.onChange(member.id);
              }}
              aria-pressed={active()}
              class={`inline-flex items-center gap-1 rounded-full border font-medium transition-colors ${pillSize()} ${
                active()
                  ? d().solid
                  : 'border-transparent bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10'
              }`}
            >
              <span>{d().icon}</span>
              <span>{d().label}</span>
              <AssigneeKindChip kind={d().kind} active={active()} />
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
          <>
            <button
              type="button"
              onClick={openCustom}
              class={`inline-flex items-center gap-1 rounded-full border border-dashed border-slate-300 font-medium text-slate-500 transition-colors hover:border-slate-400 hover:text-slate-700 dark:border-zinc-700 dark:text-zinc-400 dark:hover:border-zinc-500 dark:hover:text-zinc-200 ${pillSize()}`}
            >
              {isCustomActive() ? '✎ Edit' : '+ Custom…'}
            </button>
            <Show when={!compact()}>
              <button
                type="button"
                onClick={() =>
                  setAssigneeModalState({
                    mode: 'create',
                    onCreated: (m) => props.onChange(m.id),
                  })
                }
                class={`inline-flex items-center gap-1 rounded-full border border-sky-300/80 bg-sky-50 font-medium text-sky-700 transition-colors hover:bg-sky-100 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-300 dark:hover:bg-sky-500/20 ${pillSize()}`}
              >
                + Üye / Ajan
              </button>
            </Show>
          </>
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

// ---------------------------------------------------------------------------
// Repo, Priority & Labels helpers + pickers + badges
// ---------------------------------------------------------------------------

export function normalizeRepo(raw: string): string {
  return raw.replace(/[\u0000-\u001f\u007f]/g, '').trim().toLowerCase().slice(0, 64);
}

export function normalizeLabel(raw: string): string {
  return raw
    .replace(/[\u0000-\u001f\u007f]/g, '')
    .trim()
    .replace(/^#+/, '')
    .trim()
    .toLowerCase()
    .slice(0, 40);
}

export function normalizeLabels(items: readonly string[] | null | undefined): string[] {
  if (!items) return [];
  const out: string[] = [];
  for (const item of items) {
    for (const part of item.split(',')) {
      const cleaned = normalizeLabel(part);
      if (cleaned && !out.includes(cleaned)) out.push(cleaned);
    }
  }
  return out.slice(0, 20);
}

export function RepoBadge(props: { repo: string }) {
  const project = createMemo(() => board.projects.find((p) => p.id === normalizeRepo(props.repo)));
  const label = () => project()?.name ?? props.repo;
  const titleText = () =>
    project()?.description ? `${label()} — ${project()!.description}` : `Project: ${label()}`;
  return (
    <span
      title={titleText()}
      class="inline-flex h-[18px] max-w-[132px] items-center gap-1 rounded-[3px] border border-slate-200 bg-slate-100/80 px-1.5 font-mono text-[10.5px] font-medium text-slate-700 dark:border-zinc-700 dark:bg-zinc-700/60 dark:text-zinc-300"
    >
      <svg viewBox="0 0 16 16" class="h-2.5 w-2.5 shrink-0 opacity-70" fill="currentColor" aria-hidden="true">
        <path d="M2 2.5A2.5 2.5 0 0 1 4.5 0h8.75a.75.75 0 0 1 .75.75v12.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1 0-1.5h1.75v-2h-8a1 1 0 0 0-.714 1.7.75.75 0 1 1-1.072 1.05A2.495 2.495 0 0 1 2 11.5Zm10.5-1h-8a1 1 0 0 0-1 1v6.708A2.486 2.486 0 0 1 4.5 9h8Z" />
      </svg>
      <span class="truncate">{label()}</span>
    </span>
  );
}

export function priorityMeta(priority: number) {
  if (priority >= 10) {
    return {
      label: 'Urgent',
      short: 'P0 · Urgent',
      pill: 'bg-rose-100 text-rose-800 border-rose-200 dark:bg-rose-500/15 dark:text-rose-300 dark:border-rose-500/25',
      solid: 'border-rose-600 bg-rose-600 text-white dark:border-rose-500 dark:bg-rose-500',
    };
  }
  if (priority >= 8) {
    return {
      label: 'High',
      short: 'P1 · High',
      pill: 'bg-amber-100 text-amber-800 border-amber-200 dark:bg-amber-500/15 dark:text-amber-300 dark:border-amber-500/25',
      solid: 'border-amber-600 bg-amber-600 text-white dark:border-amber-500 dark:bg-amber-500',
    };
  }
  if (priority >= 5) {
    return {
      label: 'Normal',
      short: 'P2 · Normal',
      pill: 'bg-sky-100 text-sky-800 border-sky-200 dark:bg-sky-500/15 dark:text-sky-300 dark:border-sky-500/25',
      solid: 'border-sky-600 bg-sky-600 text-white dark:border-sky-500 dark:bg-sky-500',
    };
  }
  return {
    label: priority > 0 ? `P${priority}` : 'Low',
    short: priority > 0 ? `P${priority}` : 'Low',
    pill: 'bg-slate-100 text-slate-600 border-slate-200 dark:bg-zinc-700/60 dark:text-zinc-300 dark:border-zinc-600',
    solid: 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900',
  };
}

export function PriorityBadge(props: { priority: number }) {
  const meta = () => priorityMeta(props.priority);
  return (
    <span
      title={`Priority: ${meta().label} (${props.priority})`}
      class={`inline-flex h-[18px] items-center gap-0.5 rounded-[3px] border px-1.5 text-[10.5px] font-semibold tracking-tight ${meta().pill}`}
    >
      <Show when={props.priority >= 10}>
        <span aria-hidden="true">🔥</span>
      </Show>
      <Show when={props.priority >= 8 && props.priority < 10}>
        <span aria-hidden="true">▲</span>
      </Show>
      <span>{meta().short}</span>
    </span>
  );
}

const LABEL_PILL_STYLES = [
  'bg-violet-50 text-violet-700 border-violet-200/80 dark:bg-violet-500/10 dark:text-violet-300 dark:border-violet-400/20',
  'bg-cyan-50 text-cyan-700 border-cyan-200/80 dark:bg-cyan-500/10 dark:text-cyan-300 dark:border-cyan-400/20',
  'bg-pink-50 text-pink-700 border-pink-200/80 dark:bg-pink-500/10 dark:text-pink-300 dark:border-pink-400/20',
  'bg-lime-50 text-lime-800 border-lime-200/80 dark:bg-lime-500/10 dark:text-lime-300 dark:border-lime-400/20',
  'bg-indigo-50 text-indigo-700 border-indigo-200/80 dark:bg-indigo-500/10 dark:text-indigo-300 dark:border-indigo-400/20',
];

export function labelStyleFor(label: string): string {
  let hash = 0;
  for (let i = 0; i < label.length; i++) hash = (hash * 31 + label.charCodeAt(i)) >>> 0;
  return LABEL_PILL_STYLES[hash % LABEL_PILL_STYLES.length];
}

export function LabelPill(props: { label: string }) {
  return (
    <span
      class={`inline-flex h-[18px] max-w-[140px] items-center truncate rounded-full border px-1.5 text-[10.5px] font-medium ${labelStyleFor(props.label)}`}
    >
      #{props.label}
    </span>
  );
}

export function formatPrBadgeLabel(url: string): string {
  const m = url.match(/\/pull\/(\d+)(?:[/?#]|$)/i);
  if (m) return `PR #${m[1]}`;
  const mr = url.match(/\/merge_requests\/(\d+)(?:[/?#]|$)/i);
  if (mr) return `MR !${mr[1]}`;
  return 'PR';
}

export function PrBadge(props: { url: string }) {
  return (
    <a
      href={props.url}
      target="_blank"
      rel="noopener noreferrer"
      title={`Open pull request: ${props.url}`}
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      draggable={false}
      class="inline-flex h-[22px] items-center gap-1 rounded-full border border-emerald-200 bg-emerald-50 px-2 text-[11px] font-semibold text-emerald-700 transition-colors hover:bg-emerald-100 dark:border-emerald-500/30 dark:bg-emerald-500/10 dark:text-emerald-300 dark:hover:bg-emerald-500/20"
    >
      <svg viewBox="0 0 16 16" class="h-3 w-3 shrink-0" fill="currentColor" aria-hidden="true">
        <path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354ZM3.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm0 9.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm8.25.75a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z" />
      </svg>
      <span>{formatPrBadgeLabel(props.url)}</span>
    </a>
  );
}

export function RepoPicker(props: {
  value: string | null | undefined;
  onChange: (next: string | null) => void;
  size?: 'sm' | 'md';
}) {
  const [editingCustom, setEditingCustom] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  let customInput: HTMLInputElement | undefined;

  const currentKey = () => (props.value ? normalizeRepo(props.value) : null);
  const isKnown = (r: string | null) => !!r && board.projects.some((p) => p.id === r);
  const isCustomActive = () => !!currentKey() && !isKnown(currentKey());
  const compact = () => props.size !== 'md';
  const pillSize = () => (compact() ? 'h-6 px-2 text-[11px]' : 'h-7 px-2.5 text-xs');

  const openCustom = () => {
    setDraft(isCustomActive() ? currentKey()! : '');
    setEditingCustom(true);
    queueMicrotask(() => {
      customInput?.focus();
      customInput?.select();
    });
  };

  const commitCustom = () => {
    if (!editingCustom()) return;
    const cleaned = normalizeRepo(draft());
    setEditingCustom(false);
    if (cleaned && cleaned !== currentKey()) props.onChange(cleaned);
  };

  return (
    <div class="flex flex-wrap items-center gap-1.5">
      <For each={board.projects}>
        {(proj) => {
          const active = () => currentKey() === proj.id && !editingCustom();
          return (
            <button
              type="button"
              title={proj.description ? `${proj.name} — ${proj.description}` : proj.name}
              onClick={() => {
                setEditingCustom(false);
                props.onChange(active() ? null : proj.id);
              }}
              aria-pressed={active()}
              class={`inline-flex items-center gap-1 rounded-full border font-mono font-medium transition-colors ${pillSize()} ${
                active()
                  ? 'border-slate-700 bg-slate-700 text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900'
                  : 'border-transparent bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10'
              }`}
            >
              <span>{proj.name}</span>
            </button>
          );
        }}
      </For>

      <Show when={!editingCustom() && isCustomActive() ? currentKey() : null}>
        {(repo) => (
          <span
            class={`inline-flex max-w-[168px] items-center gap-1 rounded-full border border-slate-700 bg-slate-700 font-mono font-medium text-white dark:border-zinc-200 dark:bg-zinc-200 dark:text-zinc-900 ${pillSize()}`}
          >
            <button type="button" onClick={openCustom} title="Edit repo" class="truncate">
              {repo()}
            </button>
            <button
              type="button"
              aria-label="Clear repo"
              onClick={() => props.onChange(null)}
              class="ml-0.5 rounded-full opacity-75 hover:opacity-100"
            >
              ×
            </button>
          </span>
        )}
      </Show>

      <Show
        when={editingCustom()}
        fallback={
          <>
            <button
              type="button"
              onClick={openCustom}
              class={`inline-flex items-center gap-1 rounded-full border border-dashed border-slate-300 font-medium text-slate-500 transition-colors hover:border-slate-400 hover:text-slate-700 dark:border-zinc-700 dark:text-zinc-400 dark:hover:border-zinc-500 dark:hover:text-zinc-200 ${pillSize()}`}
            >
              {isCustomActive() ? '✎ Edit' : '+ Custom…'}
            </button>
            <Show when={!compact()}>
              <button
                type="button"
                onClick={() =>
                  setProjectModalState({
                    mode: 'create',
                    onCreated: (p) => props.onChange(p.id),
                  })
                }
                class={`inline-flex items-center gap-1 rounded-full border border-sky-300/80 bg-sky-50 font-medium text-sky-700 transition-colors hover:bg-sky-100 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-300 dark:hover:bg-sky-500/20 ${pillSize()}`}
              >
                + Proje
              </button>
            </Show>
          </>
        }
      >
        <div class="inline-flex items-center gap-1">
          <input
            ref={customInput}
            type="text"
            maxLength={64}
            value={draft()}
            placeholder="repo name"
            aria-label="Custom repo name"
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
            class={`w-32 rounded-full border border-sky-400 bg-white px-2.5 font-mono text-slate-800 outline-none focus:ring-2 focus:ring-sky-500/30 dark:border-sky-500 dark:bg-zinc-800 dark:text-zinc-100 ${
              compact() ? 'h-6 text-[11px]' : 'h-7 text-xs'
            }`}
          />
          <button
            type="button"
            aria-label="Apply custom repo"
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

export function PriorityPicker(props: {
  value: number;
  onChange: (next: number) => void;
  size?: 'sm' | 'md';
}) {
  const compact = () => props.size !== 'md';
  const pillSize = () => (compact() ? 'h-6 px-2 text-[11px]' : 'h-7 px-2.5 text-xs');
  const isPreset = () => PRIORITY_OPTIONS.some((o) => o.value === props.value);

  return (
    <div class="flex flex-wrap items-center gap-1.5">
      <For each={PRIORITY_OPTIONS}>
        {(opt) => {
          const active = () => props.value === opt.value;
          const meta = priorityMeta(opt.value);
          return (
            <button
              type="button"
              onClick={() => props.onChange(opt.value)}
              aria-pressed={active()}
              class={`inline-flex items-center gap-1 rounded-full border font-medium transition-colors ${pillSize()} ${
                active()
                  ? meta.solid
                  : 'border-transparent bg-black/5 text-slate-600 hover:bg-black/10 dark:bg-white/5 dark:text-zinc-400 dark:hover:bg-white/10'
              }`}
            >
              <span>{opt.label}</span>
              <span class="opacity-70 tabular-nums">({opt.value})</span>
            </button>
          );
        }}
      </For>
      <Show when={!isPreset()}>
        <span
          class={`inline-flex items-center rounded-full border font-medium ${pillSize()} ${priorityMeta(props.value).solid}`}
        >
          Custom ({props.value})
        </span>
      </Show>
    </div>
  );
}

export function LabelsInput(props: {
  value: readonly string[];
  onChange: (next: string[]) => void;
  placeholder?: string;
}) {
  const [draft, setDraft] = createSignal('');
  let inputEl: HTMLInputElement | undefined;

  const current = () => normalizeLabels(props.value);

  const addFromText = (raw: string) => {
    const added = normalizeLabels([raw]);
    if (added.length === 0) return;
    const merged = normalizeLabels([...current(), ...added]);
    if (merged.length !== current().length) props.onChange(merged);
  };

  const removeLabel = (label: string) => {
    props.onChange(current().filter((l) => l !== label));
  };

  const commitDraft = () => {
    const text = draft().trim();
    if (!text) {
      setDraft('');
      return;
    }
    addFromText(text);
    setDraft('');
  };

  return (
    <div
      onClick={() => inputEl?.focus()}
      class="flex min-h-[36px] flex-wrap items-center gap-1.5 rounded-[4px] border border-slate-300 bg-white px-2 py-1.5 focus-within:border-sky-500 focus-within:ring-2 focus-within:ring-sky-500/30 dark:border-zinc-700 dark:bg-zinc-800"
    >
      <For each={current()}>
        {(label) => (
          <span
            class={`inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11px] font-medium ${labelStyleFor(label)}`}
          >
            <span>#{label}</span>
            <button
              type="button"
              aria-label={`Remove ${label}`}
              onClick={(e) => {
                e.stopPropagation();
                removeLabel(label);
              }}
              class="rounded-full opacity-70 hover:opacity-100"
            >
              ×
            </button>
          </span>
        )}
      </For>
      <input
        ref={inputEl}
        type="text"
        value={draft()}
        placeholder={current().length === 0 ? (props.placeholder ?? 'Add labels (#frontend, bug)…') : 'Add label…'}
        aria-label="Labels"
        onInput={(e) => {
          const val = e.currentTarget.value;
          if (val.includes(',')) {
            const parts = val.split(',');
            const tail = parts.pop() ?? '';
            const head = parts.join(',');
            if (head.trim()) addFromText(head);
            setDraft(tail);
            e.currentTarget.value = tail;
          } else {
            setDraft(val);
          }
        }}
        onBlur={commitDraft}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
            commitDraft();
            return;
          }
          if (e.key === 'Enter' || e.key === ',') {
            if (draft().trim()) {
              e.preventDefault();
              e.stopPropagation();
              commitDraft();
            } else if (e.key === ',') {
              e.preventDefault();
            }
          } else if (e.key === 'Backspace' && !draft() && current().length > 0) {
            removeLabel(current()[current().length - 1]);
          }
        }}
        class="min-w-[130px] flex-1 bg-transparent text-xs text-slate-800 outline-none placeholder:text-slate-400 dark:text-zinc-100 dark:placeholder:text-zinc-500"
      />
    </div>
  );
}
