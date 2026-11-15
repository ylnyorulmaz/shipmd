export type Tag = 'build' | 'sell' | 'ops';
export type BuiltinAssignee = 'human' | 'antigravity' | 'kimi' | 'hermes' | 'agent';
export type Assignee = BuiltinAssignee | (string & {});

export const ASSIGNEE_PRESETS: readonly BuiltinAssignee[] = ['human', 'antigravity', 'kimi', 'hermes', 'agent'];

export const REPO_PRESETS = ['shipmd', 'ceviz-ai', 'antisocial', 'kahvefalim'] as const;

export const PRIORITY_OPTIONS = [
  { value: 0, label: 'Low', short: 'Low' },
  { value: 5, label: 'Normal', short: 'Normal' },
  { value: 8, label: 'High', short: 'High' },
  { value: 10, label: 'Urgent', short: 'Urgent' },
] as const;

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
  repo?: string | null;
  labels?: string[];
  position: number;
  updated_at: string;
}

export interface Column {
  id: string;
  title: string;
  position: number;
}

export interface ProjectLink {
  label: string;
  url: string;
}

export interface Project {
  id: string;
  name: string;
  description: string | null;
  links: ProjectLink[];
  updated_at: string;
}

export interface ProjectInput {
  id?: string;
  name: string;
  description?: string | null;
  links?: ProjectLink[];
}

export type ProjectPatch = Partial<Pick<ProjectInput, 'name' | 'description' | 'links'>>;

export type AssigneeKind = 'human' | 'ai';

export interface AssigneeMember {
  id: string;
  name: string;
  kind: AssigneeKind;
  role_description: string | null;
  avatar_badge: string | null;
  updated_at: string;
}

export interface AssigneeInput {
  id?: string;
  name: string;
  kind: AssigneeKind;
  role_description?: string | null;
  avatar_badge?: string | null;
}

export type AssigneePatch = Partial<Pick<AssigneeInput, 'name' | 'kind' | 'role_description' | 'avatar_badge'>>;

export interface BoardResponse {
  title: string;
  columns: (Column & { cards: Card[] })[];
  projects?: Project[];
  assignees?: AssigneeMember[];
}

export type CardPatch = Partial<
  Pick<Card, 'column_id' | 'position' | 'title' | 'tag' | 'assignee' | 'priority' | 'repo' | 'labels' | 'pr_url'>
> & {
  description?: string;
};

export type WsEvent =
  | { type: 'CARD_CREATED' | 'CARD_MOVED' | 'CARD_UPDATED'; card: Card; origin: string | null }
  | { type: 'CARD_DELETED'; id: string; origin: string | null }
  | { type: 'PROJECT_CREATED' | 'PROJECT_UPDATED'; project: Project; origin: string | null }
  | { type: 'PROJECT_DELETED'; id: string; origin: string | null }
  | { type: 'ASSIGNEE_CREATED' | 'ASSIGNEE_UPDATED'; assignee: AssigneeMember; origin: string | null }
  | { type: 'ASSIGNEE_DELETED'; id: string; origin: string | null }
  | { type: 'HELLO' }
  | { type: 'RESYNC' };

export const TAGS: Tag[] = ['build', 'sell', 'ops'];
