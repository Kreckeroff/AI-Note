import {useEffect, useRef, useState, type KeyboardEvent, type SyntheticEvent} from 'react';
import {Icon} from './Icon';
import type {Space, WorkspaceModel} from './model';
import {spacePath, spacesPath, withOptions} from './routes';

function fold(text: string): string {
  return text.normalize('NFC').toLowerCase().normalize('NFC');
}

const preferredCount = 4;
const groups = [
  {title: 'Мои пространства', owned: true},
  {title: 'Доступные пространства', owned: false},
] as const;

export function WorkspacePicker({workspace, current}: {workspace: WorkspaceModel; current?: Space}) {
  const {tenant, tenants, availableSpaces, catalog: {options}} = workspace;
  const [query, setQuery] = useState('');
  const search = useRef<HTMLInputElement>(null);
  const tenantMenu = useRef<HTMLDetailsElement>(null);
  const spaceMenu = useRef<HTMLDetailsElement>(null);
  const label = current?.name ?? 'Выбрать пространство';
  const filter = fold(query.trim());
  const visibleGroups = groups.map(({title, owned}) => ({
    title,
    spaces: availableSpaces.filter(space => space.isOwner === owned).filter((space, index) => (
      filter ? fold(space.name).includes(filter) : index < preferredCount || space.id === current?.id
    )),
  }));

  useEffect(() => {
    function closeOutside(event: MouseEvent) {
      for (const menu of [tenantMenu.current, spaceMenu.current]) {
        if (menu?.open && event.target instanceof Node && !menu.contains(event.target)) {
          menu.open = false;
        }
      }
    }

    document.addEventListener('click', closeOutside);
    return () => document.removeEventListener('click', closeOutside);
  }, []);

  function closeOnEscape(event: KeyboardEvent<HTMLDetailsElement>) {
    if (event.key !== 'Escape' || !event.currentTarget.open) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.open = false;
    event.currentTarget.querySelector('summary')?.focus();
  }

  function handleToggle(event: SyntheticEvent<HTMLDetailsElement>) {
    if (!event.currentTarget.open) return;

    if (event.currentTarget === spaceMenu.current) {
      if (tenantMenu.current) tenantMenu.current.open = false;
      setQuery('');
      search.current?.focus();
    } else if (spaceMenu.current) {
      spaceMenu.current.open = false;
    }
  }

  return (
    <div className="dock-identity">
      {tenants.length < 2 ? (
        <span className="dock-tenant"><Icon /><span>{tenant.name}</span></span>
      ) : (
        <details ref={tenantMenu} className="dock-choice" data-workspace-choice
          onToggle={handleToggle} onKeyDown={closeOnEscape}>
          <summary className="dock-tenant" aria-label={`Сменить среду. Текущая: ${tenant.name}`}>
            <Icon /><span>{tenant.name}</span><Icon name="down" />
          </summary>
          <div className="dock-panel">
            <strong>Выбрать среду</strong>
            <ul>
              {tenants.map(item => (
                <li key={item.id}>
                  {item.id === tenant.id ? (
                    <span className="choice-current" aria-current="true">{item.name} <small>Текущая среда</small></span>
                  ) : (
                    <a href={spacesPath(item.id)}>{item.name}<Icon name="next" /></a>
                  )}
                </li>
              ))}
            </ul>
          </div>
        </details>
      )}
      <span className="dock-divider"><Icon name="next" /></span>
      <details ref={spaceMenu} className="dock-choice" data-workspace-choice
        onToggle={handleToggle} onKeyDown={closeOnEscape}>
        <summary className="dock-space" aria-label={current ? `Сменить пространство. Текущее: ${label}` : label}>
          <Icon name="folder" /><span>{label}</span><Icon name="down" />
        </summary>
        <div className="dock-panel dock-space-panel">
          {availableSpaces.length > 0 && (
            <div className="picker-search" data-picker-search>
              <label htmlFor="picker-query" className="sr-only">Поиск пространства в меню</label>
              <input id="picker-query" ref={search} type="search" value={query} onChange={event => setQuery(event.currentTarget.value)}
                placeholder="Найти пространство" maxLength={200} />
            </div>
          )}
          <ul className="picker-spaces">
            {visibleGroups.map(({title, spaces}) => spaces.length > 0 && (
              <li key={title} data-picker-group>
                <h3>{title}</h3>
                <ul>
                  {spaces.map(space => (
                    <li key={space.id}>
                      <a data-picker-space={space.id} href={withOptions(spacePath(tenant.id, space.id), options)}
                        aria-current={space.id === current?.id ? 'page' : undefined}>
                        <Icon name="folder" /><span><strong data-picker-label>{space.name}</strong><small>{space.ownerName}</small></span>
                        <Icon name="next" />
                      </a>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
          <p className="picker-no-results" role="status" data-picker-empty hidden={visibleGroups.some(({spaces}) => spaces.length > 0)}>
            {availableSpaces.length > 0 ? 'Пространства не найдены' : 'Доступных пространств пока нет'}
          </p>
          <a className="picker-all" href={withOptions(spacesPath(tenant.id), options)}>
            <Icon name="layers" />Все пространства<Icon name="next" />
          </a>
        </div>
      </details>
    </div>
  );
}
