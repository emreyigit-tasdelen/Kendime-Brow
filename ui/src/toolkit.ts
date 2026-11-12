// Pure helpers behind kb://tools (task 6.4). No network, no eval; everything is testable.

export function formatJson(text: string, indent = 2): string {
  return JSON.stringify(JSON.parse(text), null, indent);
}

export function minifyJson(text: string): string {
  return JSON.stringify(JSON.parse(text));
}

/** Dot/bracket path query: `a.b[0].c`, `items[2]`, `$` or empty = whole document. */
export function queryJson(text: string, path: string): unknown {
  let value: unknown = JSON.parse(text);
  const p = path.trim().replace(/^\$\.?/, '');
  if (!p) return value;
  const tokens = [...p.matchAll(/([^.[\]]+)|\[(\d+)\]/g)].map((m) => (m[2] !== undefined ? Number(m[2]) : m[1]));
  for (const t of tokens) {
    if (value === null || typeof value !== 'object') throw new Error(`Cannot read "${String(t)}" of a non-object`);
    value = (value as Record<string | number, unknown>)[t];
    if (value === undefined) throw new Error(`Path not found at "${String(t)}"`);
  }
  return value;
}

export interface RegexMatch {
  index: number;
  text: string;
  groups: (string | undefined)[];
  named: Record<string, string>;
}

export function regexMatches(pattern: string, flags: string, input: string, limit = 500): RegexMatch[] {
  const re = new RegExp(pattern, flags.includes('g') ? flags : flags + 'g');
  const out: RegexMatch[] = [];
  for (const m of input.matchAll(re)) {
    out.push({ index: m.index ?? 0, text: m[0], groups: m.slice(1), named: { ...(m.groups ?? {}) } });
    if (out.length >= limit) break;
    if (m[0] === '') re.lastIndex++;
  }
  return out;
}

const toHex = (buf: ArrayBuffer) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');

export async function digest(algorithm: 'SHA-1' | 'SHA-256' | 'SHA-384' | 'SHA-512', text: string): Promise<string> {
  return toHex(await crypto.subtle.digest(algorithm, new TextEncoder().encode(text)));
}

export function base64Encode(text: string, urlSafe = false): string {
  const bytes = new TextEncoder().encode(text);
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  const b64 = btoa(bin);
  return urlSafe ? b64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '') : b64;
}

export function base64Decode(b64: string): string {
  const normalized = b64.trim().replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized + '='.repeat((4 - (normalized.length % 4)) % 4);
  const bin = atob(padded);
  return new TextDecoder('utf-8', { fatal: true }).decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
}

export function urlEncode(text: string): string {
  return encodeURIComponent(text);
}

export function urlDecode(text: string): string {
  return decodeURIComponent(text.replace(/\+/g, ' '));
}

export interface Jwt {
  header: unknown;
  payload: unknown;
  signature: string;
  expires?: string;
  expired?: boolean;
}

/** Decodes a JWT WITHOUT verifying its signature (display only). */
export function decodeJwt(token: string, now = Date.now()): Jwt {
  const parts = token.trim().split('.');
  if (parts.length !== 3) throw new Error('A JWT has three dot-separated parts');
  const header = JSON.parse(base64Decode(parts[0]));
  const payload = JSON.parse(base64Decode(parts[1]));
  const exp = typeof payload?.exp === 'number' ? payload.exp * 1000 : undefined;
  return {
    header,
    payload,
    signature: parts[2],
    expires: exp ? new Date(exp).toISOString() : undefined,
    expired: exp ? exp < now : undefined,
  };
}
