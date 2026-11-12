import { For, onCleanup, onMount, Show } from 'solid-js';
import BoardColumn from './components/BoardColumn';
import CardModal from './components/CardModal';
import Header from './components/Header';
import { board, composerColumn, connectSocket, loadBoard, openCardId, setComposerColumn, toast } from './store';

const isTyping = (t: EventTarget | null) =>
  t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));

export default function App() {
  onMount(() => {
    void loadBoard();
    connectSocket();
  });

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (composerColumn()) setComposerColumn(null);
      return;
    }
    if ((e.key === 'n' || e.key === 'N') && !e.metaKey && !e.ctrlKey && !e.altKey && !isTyping(e.target) && !openCardId()) {
      e.preventDefault();
      setComposerColumn(board.columns[0]?.id ?? null);
    }
  };
  window.addEventListener('keydown', onKey);
  onCleanup(() => window.removeEventListener('keydown', onKey));

  return (
    <div class="flex h-dvh flex-col bg-[#f7f8fa] text-slate-800 dark:bg-zinc-950 dark:text-zinc-100">
      <Header />
      <main class="flex-1 overflow-x-auto overflow-y-hidden">
        <Show
          when={board.loaded}
          fallback={
            <div class="flex gap-3 p-4">
              <For each={[0, 1, 2, 3]}>
                {() => <div class="h-48 w-[272px] shrink-0 animate-pulse rounded-md bg-[#ebecf0] dark:bg-zinc-900" />}
              </For>
            </div>
          }
        >
          <div class="flex h-full items-start gap-3 p-4">
            <For each={board.columns}>{(col) => <BoardColumn column={col} />}</For>
          </div>
        </Show>
      </main>

      <Show when={openCardId()}>{(id) => <CardModal id={id()} />}</Show>

      <Show when={toast()}>
        <div class="toast-in fixed bottom-5 left-1/2 z-[70] -translate-x-1/2 rounded-md bg-slate-900 px-4 py-2.5 text-sm text-white shadow-lg dark:bg-zinc-100 dark:text-zinc-900">
          {toast()}
        </div>
      </Show>
    </div>
  );
}
