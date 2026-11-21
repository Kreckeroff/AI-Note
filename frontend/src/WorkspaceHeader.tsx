import {Icon} from './Icon';
import {WorkspacePicker} from './WorkspacePicker';
import {PageNotifications} from './collaboration/PageNotifications';
import {spacePath, spacesPath, withOptions} from './routes';
import type {Space, WorkspaceModel} from './model';

export function WorkspaceHeader({workspace, space, pageId, sectionId}: {workspace: WorkspaceModel; space?: Space; pageId?: string; sectionId?: string}) {
  const {tenant, catalog:{options}, accountName} = workspace;
  const base = space && spacePath(tenant.id, space.id);
  const canCreateSection = space?.isOwner || space?.grantRole === 'editor';
  const search = `/workspace/${tenant.id}/search`;
  const tools = [
    ['', 'Документы', 'docs'],
    ['/graph', 'Граф', 'graph'],
    ['/approach', 'AI-подход', 'flow'],
    ['/tasks', 'Задачи', 'board'],
  ] as const;

  return (
    <header className="workspace-topbar parity-topbar" data-workspace-tenant={tenant.id} data-recent-owner={workspace.accountId}>
      <div className="parity-history">
        <button type="button" className="parity-icon" data-workspace-history="back" aria-label="Назад" title="Назад"><Icon name="left" /></button>
        <button type="button" className="parity-icon parity-forward" data-workspace-history="forward" aria-label="Вперёд" title="Вперёд"><Icon name="arrow" /></button>
        <details className="dock-choice parity-recent" data-workspace-recent>
          <summary className="parity-icon" aria-label="Недавние документы" title="Недавние документы"><Icon name="clock" /></summary>
          <div className="dock-panel"><strong>Недавние документы</strong><ul /></div>
        </details>
      </div>
      <div className="workspace-dock">
        <WorkspacePicker workspace={workspace} current={space} />
        <nav className="dock-nav" aria-label="Рабочие разделы">
          <a href={withOptions(spacesPath(tenant.id), options)} aria-current={space ? undefined : 'page'}><Icon name="layers" /><span>Пространства</span></a>
          {base && tools.map(([suffix,label,name]) => (
            <a key={name} href={`${base}${suffix}`} aria-current={suffix === '' ? 'page' : undefined}><Icon name={name} /><span>{label}</span></a>
          ))}
        </nav>
        <div className="parity-quick">
          <a className="parity-icon" data-workspace-search href={search} aria-label="Поиск" title="Поиск · ⌘K"><Icon name="search" /></a>
          {base && canCreateSection && <details className="dock-choice">
            <summary className="parity-icon parity-create" aria-label="Создать" title="Создать"><Icon name="plus" /></summary>
            <div className="dock-panel">
              <strong>Создать</strong>
              <ul>
                <li><a href={`${base}/sections/new`}><Icon name="folder" />Раздел</a></li>
                {sectionId && <li><a href={`${base}/sections/${sectionId}/pages/new`}><Icon name="doc" />Страница</a></li>}
                {space?.isOwner && <li><a href={`${base}/tasks/new${pageId ? `?source=${pageId}` : ''}`}><Icon name="board" />Задача</a></li>}
              </ul>
            </div>
          </details>}
        </div>
      </div>
      <button type="button" className="parity-icon workspace-account-toggle" hidden
        popoverTarget="workspace-account-menu" aria-label="Меню учётной записи" title="Меню учётной записи"><Icon name="user" /></button>
      <div className="workspace-account-tools" id="workspace-account-menu">
        <button type="button" className="parity-icon" data-workspace-theme aria-label="Переключить тему" title="Переключить тему"><Icon name="sun" /></button>
        {base && (space?.isOwner || tenant.role !== 'member') && <a className="parity-icon" href={`${base}/access`} aria-label="Настроить доступ" title="Настроить доступ"><Icon name="sliders" /></a>}
        <PageNotifications />
        {accountName && <a className="parity-profile" href="/workspace/profile" aria-label={`Учётная запись · ${accountName}`} title={accountName}><Icon name="user" /></a>}
      </div>
    </header>
  );
}
