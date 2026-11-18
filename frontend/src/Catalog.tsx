import {useEffect, useState, type MouseEvent} from 'react';
import {Icon} from './Icon';
import type {CatalogOptions, Space, WorkspaceModel} from './model';
import {newSpacePath, spacePath, spacesPath, withOptions} from './routes';

export function accessLabel(space: Space): string {
  if (space.isOwner) return 'Владелец';
  if (space.grantRole === 'editor') return 'Редактирование';
  return space.tenantRole === 'member' ? 'Просмотр' : 'Просмотр по роли администратора';
}

function CatalogRows({workspace}: {workspace: WorkspaceModel}) {
  const {tenant, catalog: {rows, options, total, pages, start, end}} = workspace;
  const path = spacesPath(tenant.id);
  const {page} = options;

  return (
    <div className="catalog-table" aria-label="Список пространств">
      <div className="catalog-row catalog-columns" aria-hidden="true">
        <span>Название</span><span>Владелец</span><span>Мой доступ</span><span />
      </div>
      <ul>
        {rows.map(space => (
          <li key={space.id}>
            <a className="catalog-row" data-catalog-space={space.id}
              href={withOptions(spacePath(tenant.id, space.id), options)}>
              <span className="catalog-name">
                <span className="catalog-icon"><Icon name="folder" /></span>
                <span><strong>{space.name}</strong><small><Icon name="lock" />Закрытое</small></span>
              </span>
              <span className="catalog-owner"><span className="sr-only">Владелец: </span>{space.ownerName}</span>
              <span className="catalog-access"><span className="sr-only">Мой доступ: </span>{accessLabel(space)}</span>
              <Icon name="next" />
            </a>
          </li>
        ))}
      </ul>
      {pages > 1 && (
        <nav className="catalog-pagination" aria-label="Страницы каталога">
          <span>{start}–{end} из {total}</span>
          <div>
            {page > 1 && (
              <a className="space-action space-secondary" href={withOptions(path, {...options, page: page - 1})}
                data-catalog-nav>Назад</a>
            )}
            <span>{page} из {pages}</span>
            {page < pages && (
              <a className="space-action space-secondary" href={withOptions(path, {...options, page: page + 1})}
                data-catalog-nav>Дальше</a>
            )}
          </div>
        </nav>
      )}
    </div>
  );
}

export function Catalog({workspace}: {workspace: WorkspaceModel}) {
  const [loading, setLoading] = useState(false);
  const {tenant, catalog: {options, total}} = workspace;
  const path = spacesPath(tenant.id);
  const filtered = Boolean(options.q) || options.scope !== 'all';
  const filters: Record<CatalogOptions['scope'], string> = {
    all: tenant.role === 'member' ? 'Все доступные' : 'Все пространства',
    mine: 'Мои',
    membership: 'По членству',
  };

  useEffect(() => {
    const show = () => setLoading(false);
    window.addEventListener('pageshow', show);
    return () => window.removeEventListener('pageshow', show);
  }, []);

  function onNavigate(event: MouseEvent<HTMLElement>) {
    if (!event.defaultPrevented && event.button === 0 && !event.ctrlKey && !event.metaKey &&
        !event.shiftKey && !event.altKey && event.target instanceof Element &&
        event.target.closest('[data-catalog-nav]')) {
      setLoading(true);
    }
  }

  return (
    <section className="catalog" aria-labelledby="page-title" data-catalog onClick={onNavigate}>
      <div className="space-overview">
        <div><h1 id="page-title">Пространства</h1><p className="space-summary">{tenant.name}</p></div>
        {tenant.canCreate && (
          <a className="primary-action space-action" id="create-space" href={withOptions(newSpacePath(tenant.id), options)}>
            <Icon name="add" />Создать пространство
          </a>
        )}
      </div>
      <div className="catalog-tools">
        <form method="get" action={path} data-catalog-search onSubmit={() => setLoading(true)}>
          <label className="sr-only" htmlFor="catalog-query">Поиск по названию пространства</label>
          <input id="catalog-query" type="search" name="q" defaultValue={options.q} placeholder="Поиск по названию" maxLength={200} />
          {options.scope !== 'all' && <input type="hidden" name="scope" value={options.scope} />}
          <button className="space-action space-secondary" type="submit" aria-label="Искать"><Icon name="search" /></button>
        </form>
        <nav className="catalog-filters" aria-label="Фильтр пространств">
          {(Object.keys(filters) as CatalogOptions['scope'][]).map(scope => (
            <a key={scope} href={withOptions(path, {...options, scope, page: 1})}
              aria-current={options.scope === scope ? 'true' : undefined} data-catalog-nav>{filters[scope]}</a>
          ))}
        </nav>
      </div>
      <p className="catalog-count" role="status" data-catalog-count>{loading ? 'Загружаем пространства…' : `Найдено: ${total}`}</p>
      {total > 0 ? <CatalogRows workspace={workspace} /> : (
        <section className="space-empty catalog-empty" aria-label="Результат поиска">
          <span className="space-icon"><Icon name={filtered ? 'search' : 'layers'} /></span>
          <h2>{filtered ? 'Пространства не найдены' : 'Здесь пока нет пространств'}</h2>
          {filtered && <a className="space-action space-secondary" href={path} data-catalog-nav>Сбросить поиск и фильтр</a>}
        </section>
      )}
    </section>
  );
}
