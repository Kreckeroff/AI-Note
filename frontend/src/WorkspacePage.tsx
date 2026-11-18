import {Catalog, accessLabel} from './Catalog';
import {CreateSpaceDialog} from './CreateSpaceDialog';
import {Icon} from './Icon';
import {SectionDialog} from './SectionDialog';
import {WorkspacePicker} from './WorkspacePicker';
import type {SectionTreeModel, Space, WorkspaceModel, WorkspacePageModel} from './model';
import {newSectionPath, sectionPath, spacePath, spacesPath, withOptions} from './routes';

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
  const {sections, selected, canCreateSection} = tree;
  const catalogHref = withOptions(spacesPath(tenant.id), options);
  const spaceHref = withOptions(spacePath(tenant.id, space.id), options);
  const createSectionHref = newSectionPath(tenant.id, space.id, options, selected?.id);
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
                  <a href={withOptions(sectionPath(tenant.id, space.id, section.id), options)} data-section-id={section.id}
                    aria-current={selected?.id === section.id ? 'page' : undefined}>
                    <Icon name="folder" /><span>{section.title}</span>
                  </a>
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
            <><a href={spaceHref}>{space.name}</a><Icon name="next" /><span aria-current="page">{selected.title}</span></>
          ) : <span>{space.name}</span>}
        </nav>
        <article className="documents-article">
          <header className="space-heading">
            <span className="space-icon"><Icon name="folder" /></span>
            <div>
              <h1>{selected?.title ?? space.name}</h1>
              <div className="space-badges">
                <span><Icon name="lock" />Закрытое</span><span>{accessLabel(space)}</span>
              </div>
            </div>
          </header>
          <section className="space-empty">
            <span className="space-icon"><Icon name="layers" /></span>
            <h2>{emptyMessage}</h2>
          </section>
        </article>
      </div>
    </main>
  );
}

function WorkspaceError({page}: {
  page: Extract<WorkspacePageModel, {kind: 'catalogUnavailable' | 'spaceNotFound' | 'sectionNotFound'}>;
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
            <h1>{page.kind === 'sectionNotFound' ? 'Раздел не найден' : 'Пространство не найдено'}</h1>
            <p className="space-summary">Проверьте адрес или вернитесь в рабочую область.</p>
            <a className="space-action space-secondary" href="/workspace"><Icon name="back" />К рабочей области</a>
          </section>
        )}
      </main>
    </>
  );
}

export function WorkspacePage({page}: {page: WorkspacePageModel}) {
  if (page.kind === 'catalogUnavailable' || page.kind === 'spaceNotFound' || page.kind === 'sectionNotFound') {
    return <WorkspaceError page={page} />;
  }
  if (page.kind === 'closedSpace') {
    const {workspace, space, tree} = page;
    return (
      <>
        <WorkspaceHeader workspace={workspace} space={space} />
        <Documents workspace={workspace} space={space} tree={tree} />
        {tree.form && <SectionDialog form={tree.form} spaceName={space.name} />}
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
