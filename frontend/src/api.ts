import type {
  AssigneeInput,
  AssigneeMember,
  AssigneePatch,
  BoardResponse,
  Card,
  CardPatch,
  Project,
  ProjectInput,
  ProjectPatch,
} from './types';

const BASE: string = import.meta.env.VITE_API_URL ?? '';

export function uuid(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  // Fallback for non-secure contexts (e.g. LAN IP over http).
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, '0')).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/** Identifies this tab so it can ignore the WebSocket echo of its own mutations. */
export const CLIENT_ID = uuid();

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(BASE + path, {
    method,
    headers: { 'content-type': 'application/json', 'x-client-id': CLIENT_ID },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => null);
    throw new Error(err?.error ?? `${res.status} ${res.statusText}`);
  }
  return (res.status === 204 ? undefined : await res.json()) as T;
}

export const api = {
  board: () => req<BoardResponse>('GET', '/api/board'),
  create: (card: Omit<Card, 'updated_at'>) => req<Card>('POST', '/api/cards', card),
  patch: (id: string, patch: CardPatch) => req<Card>('PATCH', `/api/cards/${id}`, patch),
  remove: (id: string) => req<void>('DELETE', `/api/cards/${id}`),
  projects: {
    list: () => req<Project[]>('GET', '/api/projects'),
    create: (input: ProjectInput) => req<Project>('POST', '/api/projects', input),
    update: (id: string, patch: ProjectPatch) => req<Project>('PUT', `/api/projects/${encodeURIComponent(id)}`, patch),
    remove: (id: string) => req<void>('DELETE', `/api/projects/${encodeURIComponent(id)}`),
  },
  assignees: {
    list: () => req<AssigneeMember[]>('GET', '/api/assignees'),
    create: (input: AssigneeInput) => req<AssigneeMember>('POST', '/api/assignees', input),
    update: (id: string, patch: AssigneePatch) =>
      req<AssigneeMember>('PUT', `/api/assignees/${encodeURIComponent(id)}`, patch),
    remove: (id: string) => req<void>('DELETE', `/api/assignees/${encodeURIComponent(id)}`),
  },
};

export function wsUrl(): string {
  if (BASE) return BASE.replace(/^http/, 'ws') + '/ws';
  return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
}

export const apiOrigin = (): string => BASE || location.origin;
