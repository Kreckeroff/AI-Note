import {useEffect, useId, useLayoutEffect, useRef, useState} from 'react';
import {flushSync} from 'react-dom';
import {Catalog, accessLabel} from './Catalog';
import {CreateSpaceDialog} from './CreateSpaceDialog';
import {DocumentCreationDialog} from './DocumentCreationDialog';
import {Icon} from './Icon';
import {PageEditor} from './PageEditor';
import {PageRenameDialog, type PageName} from './PageRenameDialog';
import {PageAttachmentList} from './editor/PageAttachmentList';
import {WorkspaceHeader} from './WorkspaceHeader';
import {ancestors, DocumentTree} from './DocumentTree';
import {PageCoverArt} from './editor/CoverGallery';
import {PageInspector, type DiscussionReference, type InspectorTab} from './collaboration/PageInspector';
import {PageSharingDialog} from './collaboration/PageSharingDialog';
import {PageNotifications} from './collaboration/PageNotifications';
import {PagePresence, usePagePresence} from './collaboration/PagePresence';
import {readPublishedPage} from './collaboration/publishedPage';
import {discussionRange, focusLinkedFragment, scrollToRange, selectedPageAnchor, type DiscussionAnchor, type SelectionAnchor} from './collaboration/pageAnchors';
import {usePageCommand} from './collaboration/api';
import type {DocumentPage, SectionTreeModel, Space, WorkspaceModel, WorkspacePageModel} from './model';
import {newPagePath, newSectionPath, pagePath, sectionPath, spacePath, spacesPath, withOptions, withParent} from './routes';

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
  const {sections, selected, canCreateSection, selectedPage: initialPage, canCreatePage, reading: initialReading, editor, pageOnly} = tree;
  const [liveReading, setLiveReading] = useState(initialReading);
  const [livePage,setLivePage] = useState(initialPage);
  const [publishedVersion,setPublishedVersion] = useState(initialReading?.revision ?? editor?.revision);
  const [renamedPages, setRenamedPages] = useState<Record<string, PageName>>({});
  const [renameTarget, setRenameTarget] = useState<string>();
  const [renameNotice, setRenameNotice] = useState('');
  const [accessEnded,setAccessEnded] = useState(false);
  const redirecting = useRef(false);
  const reading = liveReading;
  const [reference] = useState(discussionReference);
  const [inspectorOpen, setInspectorOpen] = useState(() => Boolean(reference) ||
    Boolean(initialPage && !pageOnly && window.matchMedia('(min-width: 941px)').matches));
  const [treeOpen, setTreeOpen] = useState(false);
  const [outline, setOutline] = useState<Array<{id: string; title: string; level: number}>>([]);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>(reference ? 'comments' : 'context');
  const [shareOpen, setShareOpen] = useState(false);
  const [publicLinkStatus, setPublicLinkStatus] = useState({active: editor?.publicLinkActive, sampledAt: 0});
  const [selection, setSelection] = useState<SelectionAnchor | null>(null);
  const [anchor, setAnchor] = useState<SelectionAnchor | null>(null);
  const inspectorId = useId();
  const inspectorTrigger = useRef<HTMLButtonElement>(null);
  const shareTrigger = useRef<HTMLButtonElement>(null);
  const renameTrigger = useRef<HTMLButtonElement>(null);
  const returnInspectorFocus = useRef(false);
  const returnShareFocus = useRef(false);
  const publishedBody = useRef<HTMLDivElement>(null);
  function withLocalName(page: DocumentPage, revision?: string) {
    const name = renamedPages[page.id];
    return name && (!revision || BigInt(name.revision) > BigInt(revision)) ? {...page, title: name.title} : page;
  }
  const selectedPage = livePage && withLocalName(livePage, publishedVersion);
  const pages = tree.pages.map(page => selectedPage?.id === page.id ? selectedPage : withLocalName(page));
  const pageTo = selectedPage ? pagePath(tenant.id, space.id, selectedPage.id) : undefined;
  function openRename(page: DocumentPage, opener: HTMLButtonElement) {
    renameTrigger.current = opener;
    setRenameNotice('');
    setRenameTarget(page.id);
  }
  function closeRename() {
    setRenameTarget(undefined);
    requestAnimationFrame(() => renameTrigger.current?.focus({preventScroll: true}));
  }
  function endAccess() {
    if (!pageTo || redirecting.current) return;
    redirecting.current = true;
    flushSync(() => setAccessEnded(true));
    window.location.replace(pageTo);
  }
  const presence = usePagePresence(accessEnded ? undefined : pageTo,Boolean(editor),endAccess);
  useEffect(() => {
    if (!presence || presence.publicLinkActive === undefined) return;
    const {publicLinkActive: active, sampleStartedAt: sampledAt} = presence;
    setPublicLinkStatus(current => sampledAt < current.sampledAt ? current : {active, sampledAt});
  }, [presence]);
  const declaredHistory = editor ? editor.historyTo : reading?.historyTo;
  const historyTo = pageTo && declaredHistory === `${pageTo}/revisions` ? declaredHistory : undefined;
  const role = tree.accessRole ?? (space.isOwner ? 'owner' : historyTo ? 'edit' : 'read');
  const canShare = tree.canShare ?? space.isOwner;
  const revision = publishedVersion;
  const editTo = editor ? null : reading?.editTo;
  const catalogHref = withOptions(spacesPath(tenant.id), options);
  const spaceHref = withOptions(spacePath(tenant.id, space.id), options);
  const createSectionHref = newSectionPath(tenant.id, space.id, options, selected?.id);
  const currentPages = pages.filter(page => page.sectionId === selected?.id && !page.parentId);
  const childSections = sections.filter(section => section.parentId === selected?.id && section.parentId);
  const sectionTrail = ancestors(sections, selected);
  const pageTrail = ancestors(pages, selectedPage);
  const childSectionHref = selected ? withParent(createSectionHref, selected.id) : undefined;
  const childPageHref = selectedPage && selected && canCreatePage && (Boolean(editor) || Boolean(reading?.editTo)) ?
    withParent(withOptions(newPagePath(tenant.id, space.id, selected.id), options), selectedPage.id) : undefined;
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
          setLivePage(next.page);
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

  useLayoutEffect(() => {
    const published = publishedBody.current;
    const headings = published ? Array.from(published.querySelectorAll<HTMLElement>('h1,h2,h3,h4,h5,h6')) : [];
    setOutline(headings.map((element,index) => {
      const id = `heading-${index + 1}`;
      element.id = id;
      element.tabIndex = -1;
      return {id, title: element.textContent || 'Без названия', level: Number(element.tagName.slice(1))};
    }));
  }, [reading?.renderedHtml]);

  useEffect(() => {
    if (publishedBody.current && reading?.revision) focusLinkedFragment(publishedBody.current, reading.revision);
    const match = /^#(heading-[1-9]\d*)$/.exec(window.location.hash);
    if (!editor && match) document.getElementById(match[1])?.scrollIntoView();
  }, [editor, reading?.revision]);

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
    <main className={`documents-shell${pageOnly ? ' documents-page-only' : ''}${inspectorOpen ? ' documents-inspector-open' : ''}`}
      data-tree-open={treeOpen}>
      {!pageOnly && <aside className="documents-tree" id="document-tree" aria-label="Дерево пространства"
        onKeyDown={event => { if (event.key === 'Escape' && treeOpen) {
          setTreeOpen(false);
          document.getElementById('document-tree-toggle')?.focus();
        } }}>
        <span className="documents-tree-title"><Icon name="folder" />{space.name}</span>
        <a href={`${spacePath(tenant.id,space.id)}/search`}><Icon name="search" />Найти в пространстве</a>
        <a href={`/workspace/${tenant.id}/organization`}><Icon name="home" />Главная</a>
        <details open>
          <summary>Документы</summary>
          <DocumentTree sections={sections} pages={pages} selectedSection={selected} selectedPage={selectedPage}
            sectionHref={sectionHref} pageHref={pageHref} onRenamePage={openRename} />
        </details>
        {canCreateSection && (
          <a className={`space-action section-create-link ${sections.length ? 'space-secondary' : 'primary-action'}`}
            id="create-section" href={createSectionHref}>
            <Icon name="add" />Создать раздел
          </a>
        )}
        <div className="document-tree-footer">
          <a href="/workspace/profile"><Icon name="user" />Учётная запись</a>
          {(space.isOwner || tenant.role !== 'member') &&
            <a href={`${spacePath(tenant.id,space.id)}/access`}><Icon name="settings" />Настройки пространства</a>}
        </div>
      </aside>}
      <div className="documents-content">
        <div className="document-workspace-bar">
          {!pageOnly && <button type="button" className="space-action space-secondary document-tree-toggle" id="document-tree-toggle"
            aria-label="Дерево пространства" aria-controls="document-tree" aria-expanded={treeOpen}
            onClick={() => setTreeOpen(current => !current)}><Icon name="side" /></button>}
          {pageOnly ? <p className="documents-path">Доступ только к этой странице</p> : <nav className="documents-path" aria-label="Путь">
            <a href={catalogHref}>Пространства</a><Icon name="next" />
            {selected ? <>
              <a href={spaceHref}>{space.name}</a><Icon name="next" />
              {sectionTrail.map(section => <span key={section.id}><a href={sectionHref(section.id)}>{section.title}</a><Icon name="next" /></span>)}
              {selectedPage ? <>
                <a href={sectionHref(selected.id)}>{selected.title}</a><Icon name="next" />
                {pageTrail.map(page => <span key={page.id}><a href={pageHref(page.id)}>{page.title}</a><Icon name="next" /></span>)}
                <span aria-current="page">{selectedPage.title}</span>
              </> : <span aria-current="page">{selected.title}</span>}
            </> : <span>{space.name}</span>}
          </nav>}
          {pageTo && <div className="document-action-row">
            {presence && <PagePresence people={presence.people} />}
            {editTo && <a className="space-action space-secondary" id="edit-page" href={editTo} aria-label="Редактировать" title="Редактировать"><Icon name="edit" /><span>Редактировать</span></a>}
            {canShare && <button className="space-action primary-action" type="button" ref={shareTrigger}
              aria-label="Поделиться" title="Поделиться" aria-haspopup="dialog" onClick={() => setShareOpen(true)}><Icon name="share" /><span>Поделиться</span></button>}
            <button type="button" ref={inspectorTrigger} className="space-action space-secondary"
              aria-label="Контекст страницы" title="Контекст страницы" aria-controls={inspectorId} aria-expanded={inspectorOpen}
              onClick={() => inspectorOpen ? dismissInspector() : setInspectorOpen(true)}><Icon name="context" /></button>
          </div>}
        </div>
        <article className="documents-article">
          <header className={`space-heading${publishedCover && publishedCover.value !== 'none' ? ' page-cover-heading' : ''}`}>
            <PageCoverArt cover={publishedCover} />
            {publishedCover?.value === 'orbit' && <span className="document-cover-orbit" aria-hidden="true"><Icon name="lock" /></span>}
            <span className="space-icon"><Icon name={selectedPage ? 'file' : 'folder'} /></span>
            <div>
              <div className="document-title-row">
                <h1>{selectedPage?.title ?? selected?.title ?? space.name}</h1>
                {selectedPage && (selectedPage.canEdit ?? Boolean(editTo || editor?.canSave)) &&
                  <button type="button" className="document-tree-rename" aria-haspopup="dialog"
                    aria-label="Переименовать страницу" title="Переименовать страницу"
                    onClick={event => openRename(selectedPage, event.currentTarget)}><Icon name="edit" /></button>}
              </div>
              {!pageOnly && <div className="space-badges">
                <span><Icon name="lock" />Закрытое</span><span>{accessLabel(space)}</span>
              </div>}
            </div>
          </header>
          {!editor && publishedCover && publishedCover.value !== 'none' && revision &&
            <p className="document-published-revision">Редакция {revision}</p>}
          {selectedPage ? (
            editor ? <PageEditor key={pageTo} form={editor} pageTo={pageTo} onAccessEnded={endAccess}
              renamedAtRevision={renamedPages[selectedPage.id]?.revision}
              pageLinkSearchTo={pageOnly ? undefined : `${spacePath(tenant.id,space.id)}/search`}
              publicLinkActive={publicLinkStatus.active} /> : (
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
                {reading?.template && <details className="page-template-origin" id="page-template">
                  <summary>Исходный шаблон · {reading.template.title} · v{reading.template.revision}</summary>
                  <pre>{reading.template.source}</pre>
                </details>}
                {reading?.attachments && <PageAttachmentList files={reading.attachments} />}
                {reading && pageTo && <a className="space-action space-secondary page-create-link" href={`${pageTo}?download=markdown`}>
                  <Icon name="file" />Скачать Markdown
                </a>}
                {childPageHref && <a className="space-action space-secondary page-create-link" id="create-child-page" href={childPageHref}>
                  <Icon name="add" />Добавить дочернюю страницу
                </a>}
              </>
            )
          ) : (
            <>
              {currentPages.length || childSections.length ? (
                <ul className="section-pages-list">
                  {childSections.map(section => <li key={section.id}>
                    <a className="space-action space-secondary" href={sectionHref(section.id)}>
                      <Icon name="folder" /><span>{section.title}</span>
                    </a>
                  </li>)}
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
              {childSectionHref && canCreateSection && <a className="space-action space-secondary page-create-link"
                href={childSectionHref}><Icon name="folder" />Добавить подраздел</a>}
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
        onJump={jumpToFragment} reference={reference} outline={outline} related={pageOnly ? [] : reading?.related ?? []}
        graphTo={pageOnly ? undefined : `${spacePath(tenant.id, space.id)}/graph?node=${selectedPage.id}`}
        onHeading={id => {
          const element = document.getElementById(id);
          if (!element) return;
          if (window.matchMedia('(max-width: 940px)').matches) setInspectorOpen(false);
          requestAnimationFrame(() => {
            element.focus({preventScroll: true});
            element.scrollIntoView({block: 'start'});
            history.replaceState(history.state, '', `#${id}`);
          });
        }} />}
      {pageTo && selectedPage && shareOpen && <PageSharingDialog pageTo={pageTo} title={selectedPage.title}
        onPublicLinkChange={active => setPublicLinkStatus({active, sampledAt: performance.now()})} onDismiss={() => {
        returnShareFocus.current = true;
        setShareOpen(false);
      }} />}
      <span className="sr-only" role="status">{renameNotice}</span>
      {renameTarget && <PageRenameDialog pageTo={pagePath(tenant.id, space.id, renameTarget)} onDismiss={closeRename}
        onRename={name => {
          setRenamedPages(previous => ({...previous, [renameTarget]: name}));
          setRenameNotice(`Страница переименована. Новое название «${name.title}».`);
          closeRename();
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
        </header> : <WorkspaceHeader workspace={workspace} space={space} pageId={tree.selectedPage?.id} sectionId={tree.selected?.id} />}
        <Documents workspace={workspace} space={space} tree={tree} />
        {tree.form && <DocumentCreationDialog kind="section" form={tree.form} parentName={new URL(tree.form.action,location.origin).searchParams.has('parent') ? tree.selected?.title ?? space.name : space.name} />}
        {tree.pageForm && tree.selected && (
          <DocumentCreationDialog kind="page" form={tree.pageForm} parentName={tree.selectedPage?.title ?? tree.selected.title} />
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
