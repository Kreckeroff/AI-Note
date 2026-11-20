import {isAttachmentId} from '../editor/pageAttachments';
import {queryTo} from './api';
import type {DraftText, ScalarSplice} from './draftSplice';

export type DraftSnapshot = DraftText & {
  version: string;
  publishedRevision: string;
  lease: string;
  applied?: ScalarSplice;
  fromVersion?: string;
  steps?: ScalarSplice[];
  acknowledgedVersion?: string;
  restoreConflict?: {start:number;end:number;mine:string;theirs:string};
};

export type DraftConflict = DraftSnapshot & {start: number; end: number; mine: string; theirs: string; persistent: boolean};
export type DraftCommand = {
  action: 'apply'; lease: string; operationId: string; version: string;
  start: number; end: number; text: string; format: DraftText['format']; resolve?: true;
};

export type DraftReply =
  | {kind: 'snapshot'; snapshot: DraftSnapshot}
  | {kind: 'unchanged'}
  | {kind: 'conflict'; conflict: DraftConflict}
  | {kind: 'expired'}
  | {kind: 'baseExpired'; snapshot: DraftSnapshot}
  | {kind: 'denied'}
  | {kind: 'retry'}
  | {kind: 'rejected'; message: string};

type JsonObject = Record<string, unknown>;

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function isRevision(value: unknown): value is string {
  return typeof value === 'string' && /^(0|[1-9]\d{0,18})$/u.test(value);
}

function isRange(value: JsonObject): value is JsonObject & {start: number; end: number} {
  return typeof value.start === 'number' && Number.isSafeInteger(value.start) && value.start >= 0 &&
    typeof value.end === 'number' && Number.isSafeInteger(value.end) && value.end >= value.start;
}

function isStep(value: unknown): value is ScalarSplice {
  return isObject(value) && isRange(value) && typeof value.text==='string';
}

function isFragment(value: unknown): value is NonNullable<DraftSnapshot['restoreConflict']> {
  return isObject(value) && isRange(value) && typeof value.mine==='string' && typeof value.theirs==='string';
}

function isSnapshot(value: JsonObject): value is JsonObject & DraftSnapshot {
  return typeof value.source === 'string' && (value.format === 'literal' || value.format === 'markdown') &&
    isRevision(value.version) && isRevision(value.publishedRevision) && isAttachmentId(value.lease) &&
    (value.applied == null || isStep(value.applied)) &&
    (value.acknowledgedVersion==null || isRevision(value.acknowledgedVersion)) &&
    (value.restoreConflict==null || isFragment(value.restoreConflict)) &&
    (value.steps==null || Array.isArray(value.steps) && isRevision(value.fromVersion) && value.steps.every(isStep));
}

async function requestDraft(url: string, signal: AbortSignal, command?: DraftCommand | {action: 'open'}): Promise<DraftReply> {
  let response;
  try {
    response = await fetch(url, {
      signal: AbortSignal.any([signal,AbortSignal.timeout(8000)]), cache: 'no-store', credentials: 'same-origin', redirect: 'error',
      method: command ? 'POST' : 'GET',
      headers: command ? {Accept: 'application/json', 'Content-Type': 'application/json'} : {Accept: 'application/json'},
      ...(command && {body: JSON.stringify(command)}),
    });
  } catch {
    return {kind: 'retry'};
  }
  if (response.status === 204) return {kind: 'unchanged'};
  if ([401, 403, 404].includes(response.status)) return {kind: 'denied'};
  if (response.status === 408 || response.status === 429 || response.status >= 500) return {kind: 'retry'};
  const value: unknown = await response.json().catch(() => null);
  if (isObject(value)) {
    if (response.ok && isSnapshot(value)) return {kind: 'snapshot', snapshot: value};
    if (response.status === 409) {
      if (value.code === 'leaseExpired') return {kind: 'expired'};
      if (value.code === 'baseExpired' && isSnapshot(value)) return {kind: 'baseExpired', snapshot:value};
      if (value.code === 'conflict' && isSnapshot(value) && isRange(value) &&
          typeof value.mine === 'string' && typeof value.theirs === 'string' && typeof value.persistent === 'boolean') {
        return {kind: 'conflict', conflict: value as DraftConflict};
      }
    }
    if (typeof value.error === 'string') return {kind: 'rejected', message: value.error};
  }
  return {kind: 'rejected', message: 'Не удалось обновить общий черновик.'};
}

export function openDraft(path: string, signal: AbortSignal) {
  return requestDraft(path, signal, {action: 'open'});
}

export function readDraft(path: string, snapshot: DraftSnapshot, signal: AbortSignal) {
  return requestDraft(queryTo(path, {lease: snapshot.lease, after: snapshot.version}), signal);
}

export function writeDraft(path: string, command: DraftCommand, signal: AbortSignal) {
  return requestDraft(path, signal, command);
}
