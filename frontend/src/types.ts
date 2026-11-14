export type Tag = 'build' | 'sell' | 'ops';
export type BuiltinAssignee = 'human' | 'antigravity' | 'hermes' | 'agent';
export type Assignee = BuiltinAssignee | (string & {});

export const ASSIGNEE_PRESETS: readonly BuiltinAssignee[] = ['human', 'antigravity', 'hermes', 'agent'];

export interface Card {
  id: string;
  column_id: string;
  title: string;
  description: string | null;
  tag: Tag;
  assignee: Assignee;
  status?: string;
  priority?: number;
  pr_url?: string | null;
  position: number;
  updated_at: string;
}

export interface Column {
  id: string;
  title: string;
  position: number;
}

export interface BoardResponse {
  title: string;
  columns: (Column & { cards: Card[] })[];
}

export type CardPatch = Partial<Pick<Card, 'column_id' | 'position' | 'title' | 'tag' | 'assignee'>> & {
  description?: string;
};

export type WsEvent =
  | { type: 'CARD_CREATED' | 'CARD_MOVED' | 'CARD_UPDATED'; card: Card; origin: string | null }
  | { type: 'CARD_DELETED'; id: string; origin: string | null }
  | { type: 'HELLO' }
  | { type: 'RESYNC' };

export const TAGS: Tag[] = ['build', 'sell', 'ops'];
