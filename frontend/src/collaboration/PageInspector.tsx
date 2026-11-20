import {useLayoutEffect, useRef, useSyncExternalStore} from 'react';
import {Icon} from '../Icon';
import {PageHistoryPanel} from '../editor/PageHistoryPanel';
import {CollaborationTabs} from './CollaborationTabs';
import {PageDiscussion} from './PageDiscussion';
import {roleLabels, type EffectiveRole} from './api';
import type {DiscussionAnchor, SelectionAnchor} from './pageAnchors';

const narrowScreen = '(max-width: 940px)';
const tabs = [['context', 'Контекст'], ['comments', 'Комментарии'], ['history', 'История']] as const;
export type InspectorTab = typeof tabs[number][0];
export type DiscussionReference = {thread: string; comment: string};

function onViewportChange(handler: () => void) {
  const media = window.matchMedia(narrowScreen);
  media.addEventListener('change', handler);
  return () => media.removeEventListener('change', handler);
}

export function PageInspector({id, pageTo, title, role, revision, pageOnly, open, tab, onTab, onDismiss,
  historyTo, editing, anchor, onClearAnchor, onJump, reference}: {
  id: string;
  pageTo: string;
  title: string;
  role: EffectiveRole;
  revision?: string;
  pageOnly: boolean;
  open: boolean;
  tab: InspectorTab;
  onTab(tab: InspectorTab): void;
  onDismiss(): void;
  historyTo?: string;
  editing: boolean;
  anchor: SelectionAnchor | null;
  onClearAnchor(): void;
  onJump(anchor: DiscussionAnchor): boolean;
  reference?: DiscussionReference;
}) {
  const narrow = useSyncExternalStore(onViewportChange, () => window.matchMedia(narrowScreen).matches, () => false);
  const dialog = useRef<HTMLDialogElement>(null);
  const shownTabs = historyTo ? tabs : tabs.slice(0, 2);
  const updateTab = useRef(tab);
  updateTab.current = tab;
  useLayoutEffect(() => {
    const element = dialog.current;
    if (!open || !element) return;
    if (narrow) element.showModal();
    else element.show();
    document.getElementById(`${id}-tab-${updateTab.current}`)?.focus({preventScroll: true});
    return () => element.close();
  }, [open, narrow, id]);

  const availableTab = tab === 'history' && !historyTo ? 'context' : tab;
  function attributes(panel: InspectorTab) {
    return {
      id: `${id}-panel-${panel}`,
      role: 'tabpanel',
      'aria-labelledby': `${id}-tab-${panel}`,
      hidden: availableTab !== panel,
      tabIndex: 0,
    };
  }

  return <aside hidden={!open} className="document-inspector-dock" aria-label="Контекст страницы">
    <dialog className="document-inspector" id={id} ref={dialog} aria-labelledby={`${id}-title`}
      onCancel={event => { event.preventDefault(); onDismiss(); }} onKeyDown={event => {
        if (event.key === 'Escape' && !narrow) { event.preventDefault(); onDismiss(); }
      }}>
      <header className="document-history-header">
        <h2 id={`${id}-title`}>Страница</h2>
        <button type="button" className="space-action space-close" aria-label="Закрыть контекст страницы"
          onClick={onDismiss}><Icon name="close" /></button>
      </header>
      <CollaborationTabs id={id} label="Контекст страницы" tabs={shownTabs} value={availableTab} onChange={onTab} />
      <section className="document-inspector-content inspector-context" {...attributes('context')}>
        <div className="inspector-property"><h3>Доступ</h3><p>{roleLabels[role]}</p>
          {pageOnly && <p>Только эта страница</p>}</div>
        <div className="inspector-property"><h3>О странице</h3><p>{title}</p>
          {revision && <p className="collaboration-hint">Редакция {revision}</p>}</div>
      </section>
      <section className="document-inspector-content" {...attributes('comments')}>
        <PageDiscussion pageTo={pageTo} active={open && availableTab === 'comments'} anchor={anchor}
          onClearAnchor={onClearAnchor} onJump={onJump} editing={editing} reference={reference} />
      </section>
      {historyTo && <section className="document-inspector-content" {...attributes('history')}>
        <PageHistoryPanel key={historyTo} historyTo={historyTo} editing={editing}
          active={open && availableTab === 'history'} />
      </section>}
    </dialog>
  </aside>;
}
