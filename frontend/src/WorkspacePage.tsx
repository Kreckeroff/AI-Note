import {useEffect, useId, useLayoutEffect, useRef, useState} from 'react';
import {flushSync} from 'react-dom';
import {Catalog, accessLabel} from './Catalog';
import {CreateSpaceDialog} from './CreateSpaceDialog';
import {DocumentCreationDialog} from './DocumentCreationDialog';
import {Icon} from './Icon';
import {PageEditor} from './PageEditor';
import {PageAttachmentList} from './editor/PageAttachmentList';
import {WorkspacePicker} from './WorkspacePicker';
import {PageCoverArt} from './editor/CoverGallery';
import {PageInspector, type DiscussionReference, type InspectorTab} from './collaboration/PageInspector';
import {PageSharingDialog} from './collaboration/PageSharingDialog';
import {PageNotifications} from './collaboration/PageNotifications';
import {PagePresence, usePagePresence} from './collaboration/PagePresence';
import {readPublishedPage} from './collaboration/publishedPage';
import {discussionRange, focusLinkedFragment, scrollToRange, selectedPageAnchor, type DiscussionAnchor, type SelectionAnchor} from './collaboration/pageAnchors';
import {usePageCommand} from './collaboration/api';
import type {SectionTreeModel, Space, WorkspaceModel, WorkspacePageModel} from './model';
import {newPagePath, newSectionPath, pagePath, sectionPath, spacePath, spacesPath, withOptions} from './routes';

function WorkspaceHeader({workspace, space}: {workspace: WorkspaceModel; space?: Space}) {
  const {tenant, catalog: {options}, accountName} = workspace;

  return (
    <header className="workspace-topbar">
      <div className="workspace-dock">
        <WorkspacePicker workspace={workspace} current={space} />
        <nav className="dock-nav" aria-label="Рабочие разделы">
          <a href={withOptions(spacesPath(tenant.id), options)} aria-current={space ? undefined : 'page'}>
            <Icon name="layers" />Пространства
          </a>
          {space && <span aria-current="page">Документы</span>}
        </nav>
      </div>
      <div className="workspace-account-tools">
        <PageNotifications />
        {accountName && <span className="space-account" title={accountName}>{accountName}</span>}
      </div>
    </header>
  );
}

function discussionReference(): DiscussionReference | undefined {
  const query = new URLSearchParams(window.location.search);
  const thread = query.get('thread') ?? '';
  const hashComment = /^#comment-(.+)$/u.exec(window.location.hash)?.[1];
  const comment = query.get('comment') ?? hashComment ?? thread;
  const id = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u;
  if (id.test(thread) && id.test(comment)) return {thread, comment};
}

function Documents({workspace, space, tree}: {workspace: WorkspaceModel; space: Space; tree: SectionTreeModel}) {
  const {tenant, catalog: {options}} = workspace;
  const {sections, selected, canCreateSection, pages, selectedPage: initialPage, canCreatePage, reading: initialReading, editor, pageOnly} = tree;
  const [liveReading, setLiveReading] = useState(initialReading);
  const [selectedPage,setSelectedPage] = useState(initialPage);
  const [publishedVersion,setPublishedVersion] = useState(initialReading?.revision ?? editor?.revision);
  const [accessEnded,setAccessEnded] = useState(false);
  const redirecting = useRef(false);
  const reading = liveReading;
  const [reference] = useState(discussionReference);
  const [inspectorOpen, setInspectorOpen] = useState(Boolean(reference));
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>(reference ? 'comments' : 'context');
  const [shareOpen, setShareOpen] = useState(false);
  const [selection, setSelection] = useState<SelectionAnchor | null>(null);
  const [anchor, setAnchor] = useState<SelectionAnchor | null>(null);
  const inspectorId = useId();
  const inspectorTrigger = useRef<HTMLButtonElement>(null);
  const shareTrigger = useRef<HTMLButtonElement>(null);
  const returnInspectorFocus = useRef(false);
  const returnShareFocus = useRef(false);
  const publishedBody = useRef<HTMLDivElement>(null);
  const pageTo = selectedPage ? pagePath(tenant.id, space.id, selectedPage.id) : undefined;
  function endAccess() {
    if (!pageTo || redirecting.current) return;
    redirecting.current = true;
    flushSync(() => setAccessEnded(true));
    window.location.replace(pageTo);
  }
  const presence = usePagePresence(accessEnded ? undefined : pageTo,Boolean(editor),endAccess);
  const declaredHistory = editor ? editor.historyTo : reading?.historyTo;
  const historyTo = pageTo && declaredHistory === `${pageTo}/revisions` ? declaredHistory : undefined;
  const role = tree.accessRole ?? (space.isOwner ? 'owner' : historyTo ? 'edit' : 'read');
  const canShare = tree.canShare ?? space.isOwner;
  const revision = publishedVersion;
  const editTo = editor ? null : reading?.editTo;
  const catalogHref = withOptions(spacesPath(tenant.id), options);
  const spaceHref = withOptions(spacePath(tenant.id, space.id), options);
  const createSectionHref = newSectionPath(tenant.id, space.id, options, selected?.id);
  const currentPages = pages.filter(page => page.sectionId === selected?.id);
  const publishedCover = editor ? null : reading?.cover;
  const sectionHref = (id: string) => withOptions(sectionPath(tenant.id, space.id, id), options);
  const pageHref = (id: string) => withOptions(pagePath(tenant.id, space.id, id), options);
  const emptyMessage = selected ? 'Этот раздел пока пуст' :
    !sections.length ? 'В пространстве пока нет разделов' :
      !pages.length ? 'Нет доступных страниц' : 'Выберите раздел в дереве';

  useEffect(() => {
    if (!pageTo || !publishedVersion || !presence?.publishedRevision ||
        BigInt(publishedVersion) >= BigInt(presence.publishedRevision)) return;
    const request = new AbortController();
    let timeout: number | undefined;
    async function refresh() {
      try {
        const next = await readPublishedPage(pageTo!,request.signal);
        if (next?.reading.revision && !request.signal.aborted) {
          setSelectedPage(next.page);
          setPublishedVersion(next.reading.revision);
          if (!editor) {
            setLiveReading(next.reading);
            setSelection(null);
          }
          return;
        }
      } catch {
        // Presence can recover independently of the page snapshot request.
      }
      if (!request.signal.aborted) timeout = window.setTimeout(() => void refresh(),4000);
    }
    void refresh();
    return () => {
      request.abort();
      if (timeout !== undefined) window.clearTimeout(timeout);
    };
  },[pageTo,editor,publishedVersion,presence?.publishedRevision]);

  useLayoutEffect(() => {
    if (!inspectorOpen && returnInspectorFocus.current) {
      inspectorTrigger.current?.focus({preventScroll: true});
      returnInspectorFocus.current = false;
    }
    if (!shareOpen && returnShareFocus.current) {
      shareTrigger.current?.focus({preventScroll: true});
      returnShareFocus.current = false;
    }
  }, [inspectorOpen, shareOpen]);

  useEffect(() => {
    if (!reading?.revision || role === 'read' || editor) return;
    const revision = reading.revision;
    function capture() {
      const root = publishedBody.current;
      setSelection(root ? selectedPageAnchor(root, window.getSelection(), revision) : null);
    }
    document.addEventListener('selectionchange', capture);
    return () => document.removeEventListener('selectionchange', capture);
  }, [reading?.revision, role, editor]);

  useEffect(() => {
    if (publishedBody.current && reading?.revision) focusLinkedFragment(publishedBody.current, reading.revision);
  }, [reading?.revision]);

  function dismissInspector() {
    returnInspectorFocus.current = true;
    setInspectorOpen(false);
  }

  function jumpToFragment(fragment: DiscussionAnchor) {
    const root = publishedBody.current;
    if (!root) {
      if (editor && pageTo && revision && fragment.start != null && fragment.end != null) {
        const url = new URL(pageTo, window.location.origin);
        url.hash = `fragment-${revision}-${fragment.start}-${fragment.end}`;
        window.open(url.href, '_blank', 'noopener');
        return true;
      }
      return false;
    }
    const range = discussionRange(root, fragment);
    if (!range) return false;
    if (window.matchMedia('(max-width: 940px)').matches) setInspectorOpen(false);
    requestAnimationFrame(() => {
      root.focus({preventScroll: true});
      scrollToRange(range);
    });
    return true;
  }

  if (accessEnded) return <main className="space-shell"><h1>Проверяем доступ к странице…</h1></main>;

  return (
    <main className={`documents-shell${pageOnly ? ' documents-page-only' : ''}${inspectorOpen ? ' documents-inspector-open' : ''}`}>
      {!pageOnly && <aside className="documents-tree" aria-label="Дерево пространства">
        <span className="documents-tree-title"><Icon name="folder" />{space.name}</span>
        <a href={catalogHref}><Icon name="layers" />Все пространства</a>
        <details open>
          <summary>Разделы</summary>
          {sections.length ? (
            <ul className="section-list">
              {sections.map(section => (
                <li key={section.id}>
                  <a href={sectionHref(section.id)} data-section-id={section.id}
                    aria-current={!selectedPage && selected?.id === section.id ? 'page' : undefined}>
                    <Icon name="folder" /><span>{section.title}</span>
                  </a>
                  {selected?.id === section.id && currentPages.length > 0 && (
                    <ul className="section-pages">
                      {currentPages.map(page => (
                        <li key={page.id}>
                          <a href={pageHref(page.id)} data-page-id={page.id}
                            aria-current={selectedPage?.id === page.id ? 'page' : undefined}>
                            <Icon name="file" /><span>{page.id===selectedPage?.id ? selectedPage.title : page.title}</span>
                          </a>
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
          ) : <p>Пока нет разделов</p>}
        </details>
        {canCreateSection && (
          <a className={`space-action section-create-link ${sections.length ? 'space-secondary' : 'primary-action'}`}
            id="create-section" href={createSectionHref}>
            <Icon name="add" />Создать раздел
          </a>
        )}
      </aside>}
      <div className="documents-content">
        {pageOnly ? <p className="documents-path">Доступ только к этой странице</p> : <nav className="documents-path" aria-label="Путь">
          <a href={catalogHref}>Пространства</a><Icon name="next" />
          {selected ? (
            <>
              <a href={spaceHref}>{space.name}</a><Icon name="next" />
              {selectedPage ? (
                <>
                  <a href={sectionHref(selected.id)}>{selected.title}</a><Icon name="next" />
                  <span aria-current="page">{selectedPage.title}</span>
                </>
              ) : <span aria-current="page">{selected.title}</span>}
            </>
          ) : <span>{space.name}</span>}
        </nav>}
        <article className="documents-article">
          <header className={`space-heading${publishedCover && publishedCover.value !== 'none' ? ' page-cover-heading' : ''}`}>
            <PageCoverArt cover={publishedCover} />
            <span className="space-icon"><Icon name={selectedPage ? 'file' : 'folder'} /></span>
            <div>
              <h1>{selectedPage?.title ?? selected?.title ?? space.name}</h1>
              {!pageOnly && <div className="space-badges">
                <span><Icon name="lock" />Закрытое</span><span>{accessLabel(space)}</span>
              </div>}
            </div>
          </header>
          {pageTo && (
            <div className="document-action-row">
              {editTo && <a className="space-action space-secondary" id="edit-page" href={editTo}>Редактировать</a>}
              <button type="button" ref={inspectorTrigger} className="space-action space-secondary"
                aria-controls={inspectorId} aria-expanded={inspectorOpen}
                onClick={() => inspectorOpen ? dismissInspector() : setInspectorOpen(true)}>
                <Icon name="context" />Контекст страницы
              </button>
              {presence && <PagePresence people={presence.people} />}
              {canShare && <button className="space-action primary-action" type="button" ref={shareTrigger}
                aria-haspopup="dialog" onClick={() => setShareOpen(true)}>Поделиться</button>}
            </div>
          )}
          {selectedPage ? (
            editor ? <PageEditor form={editor} pageTo={pageTo} onAccessEnded={endAccess} /> : (
              <>
                {reading?.renderedHtml ? (
                  // The server supplies this fragment from its allowlisted Markdown renderer.
                  <div ref={publishedBody} tabIndex={-1} className="document-text document-markdown"
                    dangerouslySetInnerHTML={{__html: reading.renderedHtml}} />
                ) : reading?.renderedHtml === null && reading.text ? (
                  <div ref={publishedBody} tabIndex={-1} className="document-text">{reading.text}</div>
                ) : <p className="document-empty">Пока здесь ничего нет.</p>}
                {selection && <button type="button" className="space-action space-secondary selection-comment"
                  onPointerDown={event => event.preventDefault()} onClick={() => {
                    setAnchor(selection);
                    setInspectorTab('comments');
                    setInspectorOpen(true);
                    setSelection(null);
                    window.getSelection()?.removeAllRanges();
                  }}><Icon name="quote" />Прокомментировать выделение</button>}
                {reading?.attachments && <PageAttachmentList files={reading.attachments} />}
              </>
            )
          ) : (
            <>
              {currentPages.length ? (
                <ul className="section-pages-list">
                  {currentPages.map(page => (
                    <li key={page.id}>
                      <a className="space-action space-secondary" href={pageHref(page.id)}>
                        <Icon name="file" /><span>{page.title}</span>
                      </a>
                    </li>
                  ))}
                </ul>
              ) : (
                <section className="space-empty">
                  <span className="space-icon"><Icon name="layers" /></span>
                  <h2>{emptyMessage}</h2>
                </section>
              )}
              {selected && canCreatePage && (
                <a className="space-action primary-action page-create-link" id="create-page"
                  href={withOptions(newPagePath(tenant.id, space.id, selected.id), options)}>
                  <Icon name="add" />Добавить страницу
                </a>
              )}
            </>
          )}
        </article>
      </div>
      {pageTo && selectedPage && <PageInspector id={inspectorId} pageTo={pageTo} title={selectedPage.title}
        role={role} revision={revision} pageOnly={Boolean(pageOnly)} historyTo={historyTo}
        open={inspectorOpen} tab={inspectorTab} onTab={setInspectorTab} onDismiss={dismissInspector}
        editing={Boolean(editor)} anchor={anchor} onClearAnchor={() => setAnchor(null)}
        onJump={jumpToFragment} reference={reference} />}
      {pageTo && selectedPage && shareOpen && <PageSharingDialog pageTo={pageTo} title={selectedPage.title} onDismiss={() => {
        returnShareFocus.current = true;
        setShareOpen(false);
      }} />}
    </main>
  );
}

function RequestPageAccess({to, initialSent}: {to: string; initialSent?: boolean}) {
  const [sent, setSent] = useState(Boolean(initialSent));
  const command = usePageCommand(to, () => setSent(true));
  return <div className="page-request-access">
    {sent ? <p role="status">Запрос принят к обработке.</p> :
      <button type="button" className="space-action primary-action" disabled={command.busy}
        onClick={() => void command.send({action: 'request'})}>Запросить доступ</button>}
    {command.error && <p className="form-error" role="alert">{command.error}</p>}
  </div>;
}

const missingResourceTitles = {
  spaceNotFound: 'Пространство не найдено',
  sectionNotFound: 'Раздел не найден',
  pageNotFound: 'Страница недоступна',
};

function WorkspaceError({page}: {
  page: Extract<WorkspacePageModel, {kind: 'catalogUnavailable' | 'documentsUnavailable' | keyof typeof missingResourceTitles}>;
}) {
  return (
    <>
      <header className="workspace-topbar">
        <div className="workspace-dock"><a className="dock-tenant" href="/workspace"><Icon />Север</a></div>
      </header>
      <main className="space-shell">
        {page.kind === 'catalogUnavailable' || page.kind === 'documentsUnavailable' ? (
          <section className="space-empty">
            <h1>{page.kind === 'catalogUnavailable' ? 'Не удалось загрузить пространства' : 'Не удалось открыть документы'}</h1>
            <a className="space-action space-secondary" href={page.retryTo}>Повторить попытку</a>
          </section>
        ) : (
          <section className="space-denied">
            <span className="space-icon"><Icon name="lock" /></span>
            <h1>{missingResourceTitles[page.kind]}</h1>
            <p className="space-summary">Проверьте адрес или вернитесь в рабочую область.</p>
            {page.kind === 'pageNotFound' && page.requestTo &&
              <RequestPageAccess to={page.requestTo} initialSent={page.requestSent} />}
            <a className="space-action space-secondary" href="/workspace"><Icon name="back" />К рабочей области</a>
          </section>
        )}
      </main>
    </>
  );
}

export function WorkspacePage({page}: {page: WorkspacePageModel}) {
  if (page.kind === 'catalogUnavailable' || page.kind === 'documentsUnavailable' || page.kind === 'spaceNotFound' ||
      page.kind === 'sectionNotFound' || page.kind === 'pageNotFound') {
    return <WorkspaceError page={page} />;
  }
  if (page.kind === 'pageEditRetry') {
    return (
      <main className="space-shell">
        <h1>Редактирование страницы</h1>
        <PageEditor form={page.editor} />
      </main>
    );
  }
  if (page.kind === 'closedSpace') {
    const {workspace, space, tree} = page;
    return (
      <>
        {tree.pageOnly ? <header className="workspace-topbar page-only-topbar">
          <span className="dock-tenant"><Icon />Север</span>
          <PageNotifications />
        </header> : <WorkspaceHeader workspace={workspace} space={space} />}
        <Documents workspace={workspace} space={space} tree={tree} />
        {tree.form && <DocumentCreationDialog kind="section" form={tree.form} parentName={space.name} />}
        {tree.pageForm && tree.selected && (
          <DocumentCreationDialog kind="page" form={tree.pageForm} parentName={tree.selected.title} />
        )}
      </>
    );
  }

  return (
    <>
      <WorkspaceHeader workspace={page.workspace} />
      <main className="space-shell"><Catalog workspace={page.workspace} /></main>
      {page.kind === 'createSpace' && <CreateSpaceDialog page={page} />}
    </>
  );
}
