// Typed client for the kb:// bridge (crates/kb-shell/src/protocol.rs).

export interface TabState {
  id: number;
  title: string;
  url: string;
  loading: boolean;
  active: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  bookmarked: boolean;
  blocked: number;
  shield: boolean;
  container: string;
}

export interface Bookmark {
  id: number;
  url: string;
  title: string;
}

export interface HistoryEntry {
  url: string;
  title: string;
  lastVisit: number;
  visitCount: number;
}

export interface FilterList {
  id: string;
  title: string;
  enabled: boolean;
  updated: number;
  rules: number;
  error: string;
}

export interface Profile {
  filters: FilterList[];
  settings: Record<string, string>;
  bookmarks: Bookmark[];
  history: HistoryEntry[];
}

export interface DownloadState {
  id: number;
  url: string;
  fileName: string;
  path: string;
  received: number;
  total: number;
  percent: number;
  speed: number;
  status: 'in-progress' | 'paused' | 'complete' | 'canceled' | 'interrupted';
}

export interface State {
  tabs: TabState[];
  downloads: DownloadState[];
  palette: boolean;
}

export interface ActionInfo {
  id: string;
  title: string;
  module: string;
}

export type Command =
  | { cmd: 'state' }
  | { cmd: 'new-tab'; url?: string; container?: string }
  | { cmd: 'close-tab'; id: number }
  | { cmd: 'activate-tab'; id: number }
  | { cmd: 'navigate'; text: string }
  | { cmd: 'back' }
  | { cmd: 'forward' }
  | { cmd: 'reload' }
  | { cmd: 'stop' }
  | { cmd: 'close-active-tab' }
  | { cmd: 'cycle-tab'; forward: boolean }
  | { cmd: 'actions'; query?: string }
  | { cmd: 'run-action'; id: string }
  | { cmd: 'set-palette'; open: boolean }
  | { cmd: 'toggle-bookmark' }
  | { cmd: 'profile'; history_query?: string }
  | { cmd: 'set-setting'; key: string; value: string }
  | { cmd: 'delete-bookmark'; id: number }
  | { cmd: 'clear-history' }
  | { cmd: 'cancel-download'; id: number }
  | { cmd: 'pause-download'; id: number }
  | { cmd: 'resume-download'; id: number }
  | { cmd: 'reveal-download'; id: number }
  | { cmd: 'clear-downloads' }
  | { cmd: 'set-filter-list'; id: string; enabled: boolean }
  | { cmd: 'update-filters' }
  | { cmd: 'toggle-shield' }
  | { cmd: 'tasks' }
  | { cmd: 'kill-process'; pid: number }
  | { cmd: 'pdf-open'; path: string }
  | { cmd: 'term-write'; id: number; data: string }
  | { cmd: 'term-resize'; id: number; cols: number; rows: number }
  | { cmd: 'term-close'; id: number }
  | { cmd: 'vault'; op: string; args?: Record<string, string> }
  | { cmd: 'sheet-preview'; path: string }
  | { cmd: 'image-meta'; path: string }
  | { cmd: 'image-strip'; path: string }
  | { cmd: 'image-data'; path: string }
  | { cmd: 'pdf-text' }
  | { cmd: 'pdf-fields' }
  | { cmd: 'pdf-fill'; path: string; values: Record<string, string> }
  | { cmd: 'pdf-extract'; path: string; pages: string }
  | { cmd: 'pdf-rotate'; path: string; pages?: string; degrees: number }
  | { cmd: 'pdf-merge'; paths: string[] }
  | { cmd: 'pdf-render'; page: number; dpi?: number };

interface QueryOptions {
  request: string;
  persistent?: boolean;
  onSuccess: (response: string) => void;
  onFailure: (code: number, message: string) => void;
}

declare global {
  interface Window {
    kbQuery?: (options: QueryOptions) => number;
    kbQueryCancel?: (id: number) => void;
  }
}

export const available = (): boolean => typeof window.kbQuery === 'function';

export function send(command: Command): Promise<string> {
  return new Promise((resolve, reject) => {
    if (!window.kbQuery) return reject(new Error('bridge unavailable'));
    window.kbQuery({
      request: JSON.stringify(command),
      onSuccess: resolve,
      onFailure: (code, message) => reject(new Error(`${code}: ${message}`)),
    });
  });
}

/** Receives the full state on every change. Returns an unsubscribe function. */
export function subscribe(onState: (state: State) => void): () => void {
  if (!window.kbQuery) return () => {};
  const id = window.kbQuery({
    request: JSON.stringify({ cmd: 'subscribe' }),
    persistent: true,
    onSuccess: (json) => onState(JSON.parse(json) as State),
    onFailure: (code, message) => console.error('subscribe failed', code, message),
  });
  return () => window.kbQueryCancel?.(id);
}

export async function actions(query: string): Promise<ActionInfo[]> {
  return JSON.parse(await send({ cmd: 'actions', query })) as ActionInfo[];
}

export async function profile(historyQuery = ''): Promise<Profile> {
  return JSON.parse(await send({ cmd: 'profile', history_query: historyQuery })) as Profile;
}
