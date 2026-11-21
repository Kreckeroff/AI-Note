import {useEffect, useId, useRef, useState} from 'react';
import {isAttachmentId} from '../editor/pageAttachments';
import {newOperationId} from './api';
import {isRevision} from './coeditingApi';

type Person = {id: string; name: string; mode: 'read' | 'edit'};
type PagePresenceSnapshot = {people: Person[]; publishedRevision: string; canEdit: boolean; publicLinkActive?: boolean};
type PagePresenceSample = PagePresenceSnapshot & {sampleStartedAt: number};

function isPerson(value: unknown): value is Person {
  return value != null && typeof value === 'object' && 'id' in value && isAttachmentId(value.id) &&
    'name' in value && typeof value.name === 'string' &&
    'mode' in value && (value.mode === 'read' || value.mode === 'edit');
}

function isPresenceSnapshot(value: unknown): value is PagePresenceSnapshot {
  return value != null && typeof value === 'object' && 'people' in value &&
    Array.isArray(value.people) && value.people.every(isPerson) &&
    'publishedRevision' in value && isRevision(value.publishedRevision) &&
    'canEdit' in value && typeof value.canEdit === 'boolean' &&
    (!('publicLinkActive' in value) || typeof value.publicLinkActive === 'boolean');
}

export function usePagePresence(path: string | undefined, editing: boolean, onDenied: () => void) {
  const tab = useRef<string | null>(null);
  const [state, setState] = useState<{path:string; presence?: PagePresenceSample}>();
  const denied = useRef(onDenied);
  denied.current = onDenied;

  useEffect(() => {
    if (!path) return;
    const href = path + '/presence';
    const tabId = tab.current ??= newOperationId();
    const request = new AbortController();
    let timer: number | undefined;
    let expiry: number | undefined;
    let pending = false;
    let hidden = false;
    const send = (mode: 'read' | 'edit' | 'leave') => fetch(href, {
      method:'POST', credentials:'same-origin', cache:'no-store', redirect:'error',
      headers:{Accept:'application/json','Content-Type':'application/json'},
      body:JSON.stringify({tabId,mode}),
      ...(mode === 'leave' ? {keepalive:true} : {
        signal:AbortSignal.any([request.signal,AbortSignal.timeout(3500)]),
      }),
    });

    function clearPresence() {
      if (expiry !== undefined) window.clearTimeout(expiry);
      setState({path:path!});
    }

    async function poll() {
      if (pending || hidden || request.signal.aborted) return;
      if (timer !== undefined) window.clearTimeout(timer);
      pending = true;
      const started = performance.now();
      try {
        const response = await send(editing ? 'edit' : 'read');
        if (request.signal.aborted || hidden) return;
        if ([401,403,404].includes(response.status)) {
          clearPresence();
          request.abort();
          denied.current();
          return;
        }
        const value: unknown = response.ok ? await response.json() : undefined;
        if (request.signal.aborted || hidden) return;
        // The server expires a tab after 10 seconds. Age the sample from the
        // request start so a slow response cannot extend visibility past 15.
        const remaining = 5000 - (performance.now() - started);
        if (isPresenceSnapshot(value) && remaining > 0) {
          if (editing && !value.canEdit) {
            clearPresence();
            request.abort();
            denied.current();
          } else {
            if (expiry !== undefined) window.clearTimeout(expiry);
            setState({path:path!,presence:{...value,sampleStartedAt:started}});
            expiry = window.setTimeout(clearPresence,remaining);
          }
        } else clearPresence();
      } catch {
        if (!request.signal.aborted && !hidden) clearPresence();
      } finally {
        pending = false;
        if (!request.signal.aborted && !hidden) timer = window.setTimeout(() => void poll(),4000);
      }
    }

    function leave() {
      if (hidden) return;
      hidden = true;
      if (timer !== undefined) window.clearTimeout(timer);
      clearPresence();
      void send('leave').catch(() => undefined);
    }
    function enter() {
      hidden = false;
      void poll();
    }

    window.addEventListener('pagehide',leave);
    window.addEventListener('pageshow',enter);
    window.addEventListener('online',enter);
    void poll();
    return () => {
      request.abort();
      leave();
      window.removeEventListener('pagehide',leave);
      window.removeEventListener('pageshow',enter);
      window.removeEventListener('online',enter);
    };
  },[path,editing]);

  return state?.path === path ? state?.presence : undefined;
}

function initials(name: string) {
  return name.trim().split(/\s+/u).slice(0,2).map(part => [...part][0] ?? '').join('').toLocaleUpperCase('ru');
}

export function PagePresence({people}: {people: readonly Person[]}) {
  const details = useRef<HTMLDetailsElement>(null);
  const heading = useId();
  if (!people.length) return null;
  return <details className="coedit-presence" ref={details} onKeyDown={event => {
    if (event.key !== 'Escape' || !details.current?.open) return;
    details.current.open = false;
    details.current.querySelector('summary')?.focus();
  }}>
    <summary className="space-action space-secondary" aria-label={`Кто на странице · коллег (${people.length})`}>
      <span className="coedit-avatar-stack" aria-hidden="true">{people.slice(0,4).map(person =>
        <span className="coedit-avatar" key={person.id}>{initials(person.name)}</span>)}
        {people.length > 4 && <span className="coedit-presence-wide">+{people.length - 4}</span>}
        {people.length > 2 && <span className="coedit-presence-narrow">+{people.length - 2}</span>}
      </span>
    </summary>
    <section className="coedit-presence-list" aria-labelledby={heading}>
      <h2 id={heading}>Кто на странице</h2>
      <ul>{people.map(person => <li key={person.id}>
        <span className="coedit-avatar" aria-hidden="true">{initials(person.name)}</span>
        <span>{person.name}</span>
        <span className="coedit-presence-mode">{person.mode==='edit' ? 'Редактирует' : 'Читает'}</span>
      </li>)}</ul>
    </section>
  </details>;
}
