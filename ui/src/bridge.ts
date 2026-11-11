// Typed client for the kb:// bridge (crates/kb-shell/src/protocol.rs).

export interface TabState {
  id: number;
  title: string;
  url: string;
  loading: boolean;
  active: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface State {
  tabs: TabState[];
}

export type Command =
  | { cmd: 'state' }
  | { cmd: 'new-tab'; url?: string }
  | { cmd: 'close-tab'; id: number }
  | { cmd: 'activate-tab'; id: number }
  | { cmd: 'navigate'; text: string }
  | { cmd: 'back' }
  | { cmd: 'forward' }
  | { cmd: 'reload' }
  | { cmd: 'stop' };

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
