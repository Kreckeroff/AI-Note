import {useEffect, useRef, useState} from 'react';
import {Icon} from '../Icon';
import {isDiscussionHref, isPageAddress, queryTo, useCollaborationQuery} from './api';

type PageNotices = {
  notifications: ({id:string; author:string; href:string; kind?:'mention'} |
    {id:string; author:string; href:string; kind:'publication'; title:string; revision:string; createdAt:number})[];
  nextOffset: number | null;
};

const publicationTime = new Intl.DateTimeFormat('ru-RU',{dateStyle:'short',timeStyle:'short'});

function PublishedAt({stamp}: {stamp: number}) {
  if (!Number.isFinite(stamp)) return null;
  const date = new Date(stamp);
  if (!Number.isFinite(date.valueOf())) return null;
  return <time dateTime={date.toISOString()}>{publicationTime.format(date)}</time>;
}

export function PageNotifications() {
  const [open, setOpen] = useState(false);
  const [offset, setOffset] = useState(0);
  const [version, setVersion] = useState(0);
  const href = queryTo('/workspace/notifications',{offset});
  const cached = useRef<{href:string; data:PageNotices} | undefined>(undefined);
  const query = useCollaborationQuery<PageNotices>(open ? href : null,version);
  if (query?.data) cached.current = {href,data:query.data};
  const result = query ?? (cached.current?.href===href ? cached.current : undefined);
  const error = query?.error;
  const data = result?.data;

  useEffect(() => {
    if (!open || offset!==0) return;
    const timer=window.setInterval(() => setVersion(value => value+1),4000);
    return () => window.clearInterval(timer);
  },[open,offset]);

  return <details className="page-notifications" open={open} onToggle={event => {
    setOpen(event.currentTarget.open);
    setOffset(0);
  }}>
    <summary className="space-action space-secondary" aria-label="Уведомления" title="Уведомления"><Icon name="bell" /><span>Уведомления</span></summary>
    <div className="page-notifications-panel">
      <h2>Уведомления</h2>
      {!result && <p role="status">Загружаем уведомления…</p>}
      {error && <p className="form-error" role="alert">{error} <button type="button"
        className="space-action space-secondary" onClick={() => setVersion(version + 1)}>Повторить</button></p>}
      {data && <>
        {data.notifications.length ? <ul>{data.notifications.map(item => {
          if (item.kind === 'publication') return isPageAddress(item.href) ? <li key={`publication-${item.id}`}>
            <a href={item.href}><strong>{item.title}</strong> · Редакция {item.revision}
              <span className="page-publication-notice">{item.author} · <PublishedAt stamp={item.createdAt} /></span>
            </a>
          </li> : null;
          return isDiscussionHref(item.href) ? <li key={`mention-${item.id}`}>
            <a href={item.href}><strong>{item.author}</strong> упоминает вас в комментарии</a>
          </li> : null;
        })}</ul> : <p>Уведомлений пока нет.</p>}
        <div className="collaboration-pager">
          {offset > 0 && <button type="button" className="space-action space-secondary"
            onClick={() => setOffset(0)}>С начала</button>}
          {data.nextOffset != null && <button type="button" className="space-action space-secondary"
            onClick={() => setOffset(data.nextOffset!)}>Дальше</button>}
        </div>
      </>}
    </div>
  </details>;
}
