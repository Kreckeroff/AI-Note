import {useEffect, useRef, useState} from 'react';
import type {DraftText, TextSplice} from './draftSplice';
import {SharedDraftSession, type SharedDraftView} from './sharedDraft';

export function useSharedDraft(path: string | undefined, preserve: DraftText | undefined, onDraft: (text: DraftText, steps?: TextSplice[]) => void,
  onDenied: () => void) {
  const [state, setState] = useState<SharedDraftView>({status:'opening',pending:0,canResolve:false,message:''});
  const transport = useRef<SharedDraftSession | null>(null);
  const callbacks = useRef({onDraft, onDenied, preserve});
  callbacks.current = {onDraft, onDenied, preserve};
  useEffect(() => {
    if (!path) return;
    const session = new SharedDraftSession(path + '/coedit', {
      onDraft: (text,steps) => callbacks.current.onDraft(text,steps),
      onDenied: () => callbacks.current.onDenied(),
      onState: setState,
    }, callbacks.current.preserve);
    transport.current = session;
    session.start();
    function warn(event: BeforeUnloadEvent) {
      if (!session.hasUnconfirmed()) return;
      event.preventDefault();
      event.returnValue = true;
    }
    window.addEventListener('beforeunload', warn);
    return () => {
      window.removeEventListener('beforeunload', warn);
      session.stop();
      transport.current = null;
    };
  }, [path]);
  return {state: path ? state : null, transport};
}
