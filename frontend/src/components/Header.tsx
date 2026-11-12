import { createMemo, createSignal, Show } from 'solid-js';
import { board, connected, doneColumnId, setComposerColumn } from '../store';

function RatioBar() {
  const stats = createMemo(() => {
    const done = doneColumnId();
    let build = 0;
    let sell = 0;
    for (const c of Object.values(board.cards)) {
      if (c.column_id !== done) continue;
      if (c.tag === 'build') build++;
      else if (c.tag === 'sell') sell++;
    }
    const total = build + sell;
    const buildPct = total ? Math.round((build / total) * 100) : 0;
    return { build, sell, total, buildPct, sellPct: total ? 100 - buildPct : 0 };
  });
  const warn = () => stats().buildPct > 75;

  return (
    <div
      class="flex min-w-0 items-center gap-3"
      title={`Shipped (Done column): ${stats().build} build · ${stats().sell} sell`}
    >
      <span class="hidden text-[11px] font-semibold tracking-wide text-slate-400 uppercase sm:inline dark:text-zinc-500">Shipped</span>
      <div
        class="relative flex h-2 w-40 overflow-hidden rounded-full bg-slate-200 sm:w-56 dark:bg-zinc-800"
        classList={{ 'ring-2 ring-amber-400/70': warn() }}
      >
        <div class="h-full bg-sky-500 transition-[width] duration-500 ease-out" style={{ width: `${stats().buildPct}%` }} />
        <div class="h-full bg-emerald-500 transition-[width] duration-500 ease-out" style={{ width: `${stats().sellPct}%` }} />
      </div>
      <Show
        when={stats().total > 0}
        fallback={<span class="text-xs text-slate-400 dark:text-zinc-500">Nothing shipped yet</span>}
      >
        <span class="text-xs whitespace-nowrap text-slate-600 tabular-nums dark:text-zinc-400">
          <span class="font-semibold text-sky-600 dark:text-sky-400">Build {stats().buildPct}%</span>
          <span class="mx-1 text-slate-300 dark:text-zinc-600">/</span>
          <span class="font-semibold text-emerald-600 dark:text-emerald-400">Sell {stats().sellPct}%</span>
        </span>
      </Show>
      <Show when={warn()}>
        <span class="hidden items-center rounded-full bg-amber-100 px-2 py-0.5 text-[11px] font-medium whitespace-nowrap text-amber-800 md:inline-flex dark:bg-amber-400/10 dark:text-amber-300">
          ⚠ Heavy on build — go sell something
        </span>
      </Show>
    </div>
  );
}

function ThemeToggle() {
  const [dark, setDark] = createSignal(document.documentElement.classList.contains('dark'));
  const toggle = () => {
    const next = !dark();
    setDark(next);
    const root = document.documentElement;
    root.classList.add('theme-switching');
    root.classList.toggle('dark', next);
    requestAnimationFrame(() => requestAnimationFrame(() => root.classList.remove('theme-switching')));
    try {
      localStorage.setItem('theme', next ? 'dark' : 'light');
    } catch {}
  };
  return (
    <button
      type="button"
      onClick={toggle}
      aria-label="Toggle dark mode"
      title="Toggle theme"
      class="grid h-8 w-8 place-items-center rounded-md text-slate-500 hover:bg-slate-100 hover:text-slate-800 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-100"
    >
      <Show
        when={dark()}
        fallback={
          <svg viewBox="0 0 20 20" class="h-4 w-4" fill="currentColor">
            <path d="M17.3 13.4A7.5 7.5 0 0 1 6.6 2.7a7.5 7.5 0 1 0 10.7 10.7Z" />
          </svg>
        }
      >
        <svg viewBox="0 0 20 20" class="h-4 w-4" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <circle cx="10" cy="10" r="3.4" />
          <path d="M10 1.8v2M10 16.2v2M1.8 10h2M16.2 10h2M4.2 4.2l1.4 1.4M14.4 14.4l1.4 1.4M4.2 15.8l1.4-1.4M14.4 5.6l1.4-1.4" />
        </svg>
      </Show>
    </button>
  );
}

export default function Header() {
  return (
    <header class="flex h-12 shrink-0 items-center gap-4 border-b border-slate-200 bg-white px-4 dark:border-zinc-800 dark:bg-zinc-950">
      <div class="flex shrink-0 items-center gap-2">
        <svg viewBox="0 0 32 32" class="h-6 w-6">
          <rect width="32" height="32" rx="6" fill="#0c66e4" />
          <rect x="6" y="6" width="8" height="18" rx="2" fill="#fff" />
          <rect x="18" y="6" width="8" height="11" rx="2" fill="#fff" />
        </svg>
        <h1 class="text-[15px] font-bold tracking-tight text-slate-800 dark:text-zinc-100">{board.title}</h1>
      </div>

      <div class="flex min-w-0 flex-1 justify-center">
        <RatioBar />
      </div>

      <div class="flex shrink-0 items-center gap-1.5">
        <span
          class="mr-1 flex items-center gap-1.5 text-[11px] font-medium text-slate-400 dark:text-zinc-500"
          title={connected() ? 'Live: real-time sync connected' : 'Offline: reconnecting…'}
        >
          <span
            class="h-2 w-2 rounded-full"
            classList={{ 'bg-emerald-500 shadow-[0_0_0_3px_rgba(16,185,129,.18)]': connected(), 'bg-slate-300 dark:bg-zinc-600': !connected() }}
          />
          <span class="hidden sm:inline">{connected() ? 'Live' : 'Offline'}</span>
        </span>
        <ThemeToggle />
        <button
          type="button"
          onClick={() => setComposerColumn(board.columns[0]?.id ?? null)}
          title="Add card (N)"
          class="flex h-8 items-center gap-1.5 rounded-md bg-sky-600 px-3 text-sm font-medium text-white shadow-sm hover:bg-sky-700 active:bg-sky-800"
        >
          <span class="text-base leading-none">+</span>
          <span class="hidden sm:inline">Add Card</span>
          <kbd class="ml-0.5 hidden rounded bg-white/20 px-1 font-sans text-[10px] sm:inline">N</kbd>
        </button>
      </div>
    </header>
  );
}
