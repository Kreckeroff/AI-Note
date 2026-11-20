import {useEffect, useRef, useState} from 'react';
import {Icon} from '../Icon';
import {collaborationError, collaborationJson} from './api';

export function PageWatch({pageTo, active}: {pageTo: string; active: boolean}) {
  const href=pageTo+'/watch';
  const [following,setFollowing]=useState<boolean>();
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState('');
  const [refresh,setRefresh]=useState(0);
  const mutation=useRef(0);
  const updating=useRef(false);

  useEffect(() => {
    if (!active) return;
    const controller=new AbortController();
    let requesting=false;
    async function read() {
      if (requesting || updating.current) return;
      requesting=true;
      const epoch=mutation.current;
      try {
        const next=await collaborationJson<{following:boolean}>(href,{signal:controller.signal});
        if (!controller.signal.aborted && epoch===mutation.current) {
          setFollowing(next.following);
          setError('');
        }
      } catch (failure) {
        if (!controller.signal.aborted && epoch===mutation.current) setError(collaborationError(failure));
      } finally {
        requesting=false;
      }
    }
    void read();
    const timer=window.setInterval(() => void read(),4000);
    return () => {
      controller.abort();
      window.clearInterval(timer);
    };
  },[href,active,refresh]);

  async function toggle() {
    if (updating.current || typeof following !== 'boolean') return;
    updating.current=true;
    mutation.current++;
    setBusy(true);
    setError('');
    try {
      const next=await collaborationJson<{following:boolean}>(href,{
        method:'POST',
        headers:{'Content-Type':'application/json'},
        body:JSON.stringify({following:!following}),
      });
      setFollowing(next.following);
    } catch (failure) {
      setError(collaborationError(failure));
    } finally {
      updating.current=false;
      setBusy(false);
    }
  }

  if (error && following === undefined) return <div className="inspector-property">
    <p role="alert">{error}</p>
    <button type="button" className="space-action space-secondary" onClick={() => setRefresh(refresh+1)}>Повторить</button>
  </div>;

  return <div className="inspector-property">
    <button type="button" aria-pressed={following ?? false}
      disabled={busy || typeof following !== 'boolean'} className="space-action space-secondary"
      onClick={() => void toggle()}>
      <Icon name="bell" />{following ? 'Отписаться от изменений' : 'Следить за изменениями'}
    </button>
    {error && <p role="alert">{error}</p>}
  </div>;
}
