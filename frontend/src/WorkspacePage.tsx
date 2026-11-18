import {Catalog, accessLabel} from './Catalog';
import {CreateSpaceDialog} from './CreateSpaceDialog';
import {DocumentCreationDialog} from './DocumentCreationDialog';
import {Icon} from './Icon';
import {WorkspacePicker} from './WorkspacePicker';
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
      {accountName && <span className="space-account" title={accountName}>{accountName}</span>}
    </header>
  );
}

function Documents({workspace, space, tree}: {workspace: WorkspaceModel; space: Space; tree: SectionTreeModel}) {
  const {tenant, catalog: {options}} = workspace;
  const {sections, selected, canCreateSection, pages, selectedPage, canCreatePage} = tree;
  const catalogHref = withOptions(spacesPath(tenant.id), options);
  const spaceHref = withOptions(spacePath(tenant.id, space.id), options);
  const createSectionHref = newSectionPath(tenant.id, space.id, options, selected?.id);
  const currentPages = pages.filter(page => page.sectionId === selected?.id);
  const sectionHref = (id: string) => withOptions(sectionPath(tenant.id, space.id, id), options);
  const pageHref = (id: string) => withOptions(pagePath(tenant.id, space.id, id), options);
  const emptyMessage = selected ? 'Этот раздел пока пуст' :
    sections.length ? 'Выберите раздел в дереве' : 'В пространстве пока нет разделов';

  return (
    <main className="documents-shell">
      <aside className="documents-tree" aria-label="Дерево пространства">
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
                            <Icon name="file" /><span>{page.title}</span>
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
      </aside>
      <div className="documents-content">
        <nav className="documents-path" aria-label="Путь">
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
        </nav>
        <article className="documents-article">
          <header className="space-heading">
            <span className="space-icon"><Icon name={selectedPage ? 'file' : 'folder'} /></span>
            <div>
              <h1>{selectedPage?.title ?? selected?.title ?? space.name}</h1>
              <div className="space-badges">
                <span><Icon name="lock" />Закрытое</span><span>{accessLabel(space)}</span>
              </div>
            </div>
          </header>
          {selectedPage ? <p className="document-empty">Пока здесь ничего нет.</p> : (
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
    </main>
  );
}

const missingResourceTitles = {
  spaceNotFound: 'Пространство не найдено',
  sectionNotFound: 'Раздел не найден',
  pageNotFound: 'Страница не найдена',
};

function WorkspaceError({page}: {
  page: Extract<WorkspacePageModel, {kind: 'catalogUnavailable' | keyof typeof missingResourceTitles}>;
}) {
  return (
    <>
      <header className="workspace-topbar">
        <div className="workspace-dock"><a className="dock-tenant" href="/workspace"><Icon />Север</a></div>
      </header>
      <main className="space-shell">
        {page.kind === 'catalogUnavailable' ? (
          <section className="space-empty">
            <h1>Не удалось загрузить пространства</h1>
            <a className="space-action space-secondary" href={page.retryTo}>Повторить попытку</a>
          </section>
        ) : (
          <section className="space-denied">
            <span className="space-icon"><Icon name="lock" /></span>
            <h1>{missingResourceTitles[page.kind]}</h1>
            <p className="space-summary">Проверьте адрес или вернитесь в рабочую область.</p>
            <a className="space-action space-secondary" href="/workspace"><Icon name="back" />К рабочей области</a>
          </section>
        )}
      </main>
    </>
  );
}

export function WorkspacePage({page}: {page: WorkspacePageModel}) {
  if (page.kind === 'catalogUnavailable' || page.kind === 'spaceNotFound' || page.kind === 'sectionNotFound' ||
      page.kind === 'pageNotFound') {
    return <WorkspaceError page={page} />;
  }
  if (page.kind === 'closedSpace') {
    const {workspace, space, tree} = page;
    return (
      <>
        <WorkspaceHeader workspace={workspace} space={space} />
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
