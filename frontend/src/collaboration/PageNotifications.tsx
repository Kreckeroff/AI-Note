import {useState} from 'react';
import {Icon} from '../Icon';
import {isDiscussionHref, queryTo, useCollaborationQuery} from './api';

type PageNotices = {
  notifications: {id: string; author: string; href: string}[];
  nextOffset: number | null;
};

export function PageNotifications() {
  const [open, setOpen] = useState(false);
  const [offset, setOffset] = useState(0);
  const [version, setVersion] = useState(0);
  const result = useCollaborationQuery<PageNotices>(
    open ? queryTo('/workspace/notifications', {offset}) : null, version,
  );
  return <details className="page-notifications" open={open} onToggle={event => {
    setOpen(event.currentTarget.open);
    setOffset(0);
  }}>
    <summary className="space-action space-secondary"><Icon name="bell" />Уведомления</summary>
    <div className="page-notifications-panel">
      <h2>Упоминания</h2>
      {!result && <p role="status">Загружаем уведомления…</p>}
      {result?.error && <p className="form-error" role="alert">{result.error} <button type="button"
        className="space-action space-secondary" onClick={() => setVersion(version + 1)}>Повторить</button></p>}
      {result?.data && <>
        {result.data.notifications.length ? <ul>{result.data.notifications.filter(item => isDiscussionHref(item.href))
          .map(item => <li key={item.id}><a href={item.href}><strong>{item.author}</strong> упоминает вас в комментарии</a></li>)}</ul>
          : <p>Упоминаний пока нет.</p>}
        <div className="collaboration-pager">
          {offset > 0 && <button type="button" className="space-action space-secondary"
            onClick={() => setOffset(0)}>С начала</button>}
          {result.data.nextOffset != null && <button type="button" className="space-action space-secondary"
            onClick={() => setOffset(result.data!.nextOffset!)}>Дальше</button>}
        </div>
      </>}
    </div>
  </details>;
}
