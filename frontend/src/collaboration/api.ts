import {useEffect, useRef, useState} from 'react';

export type PageRole = 'read' | 'comment' | 'edit';
export type EffectiveRole = PageRole | 'owner';

export const roleLabels: Record<EffectiveRole, string> = {
  owner: 'Владелец',
  read: 'Просмотр',
  comment: 'Комментирование',
  edit: 'Редактирование',
};

const uuid = '[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}';
const pageAddress = `/workspace/${uuid}/spaces/${uuid}/pages/${uuid}`;
const pagePattern = new RegExp(`^${pageAddress}$`, 'u');
const discussionPattern = new RegExp(`^${pageAddress}\\?thread=${uuid}(?:&comment=(${uuid}))?#comment-(${uuid})$`, 'u');

export function isPageAddress(path: unknown): path is string {
  return typeof path === 'string' && pagePattern.test(path);
}

export function isDiscussionHref(path: unknown): path is string {
  if (typeof path !== 'string') return false;
  const match = discussionPattern.exec(path);
  return match !== null && (!match[1] || match[1] === match[2]);
}

export function queryTo(path: string, params: Record<string, string | number>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (String(value)) query.set(key, String(value));
  }
  return query.size ? `${path}?${query}` : path;
}

class CollaborationRequestError extends Error {
  constructor(readonly status: number, message: string) {
    super(message);
  }
}

export async function collaborationJson<T>(href: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(href, {
    ...options,
    cache: 'no-store',
    credentials: 'same-origin',
    redirect: 'error',
    headers: {Accept: 'application/json', ...options.headers},
  });
  const value: unknown = await response.json().catch(error => {
    if (response.ok) throw error;
    return null;
  });
  if (!response.ok) {
    const error = value && typeof value === 'object' && 'error' in value ? value.error : null;
    throw new CollaborationRequestError(response.status, typeof error === 'string' ? error : 'Действие недоступно.');
  }
  return value as T;
}

export function collaborationError(error: unknown): string {
  if (error instanceof Error && !(error instanceof TypeError) && !(error instanceof SyntaxError)) return error.message;
  return 'Не удалось связаться с «Севером». Повторите попытку.';
}

export function useCollaborationQuery<T>(href: string | null, revision = 0, enabled = true) {
  const [result, setResult] = useState<{
    href: string; revision: number; data?: T; error?: string;
  }>();
  useEffect(() => {
    if (!href || !enabled) return;
    const controller = new AbortController();
    collaborationJson<T>(href, {signal: controller.signal}).then(
      data => { if (!controller.signal.aborted) setResult({href, revision, data}); },
      error => {
        if (!controller.signal.aborted) setResult({href, revision, error: collaborationError(error)});
      },
    );
    return () => controller.abort();
  }, [href, revision, enabled]);
  return result?.href === href && result.revision === revision ? result : undefined;
}

export function newOperationId(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function usePageCommand(href: string, onSaved?: () => void, onConflict?: () => void) {
  const operation = useRef<{payload: string; id: string} | null>(null);
  const inFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  async function send(action: Record<string, unknown>, id?: string): Promise<boolean> {
    if (inFlight.current) return false;
    const payload = JSON.stringify(action);
    const operationId = id ?? (operation.current?.payload === payload ? operation.current.id : newOperationId());
    operation.current = {payload, id: operationId};
    inFlight.current = true;
    setBusy(true);
    setError('');
    try {
      await collaborationJson(href, {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({...action, operationId}),
      });
      operation.current = null;
      onSaved?.();
      return true;
    } catch (failure) {
      setError(collaborationError(failure));
      if (failure instanceof CollaborationRequestError && failure.status === 409) onConflict?.();
      return false;
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  }

  return {send, busy, error, clearError: () => setError('')};
}
