(() => {
  const themeKey = 'sever:workspace-theme';
  const recentPrefix = 'sever:recent-documents:v2:';
  const uuid = '[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}';
  const pagePath = new RegExp('^/workspace/(' + uuid + ')/spaces/(' + uuid + ')/pages/(' + uuid + ')$');
  const validId = new RegExp('^' + uuid + '$');
  const dark = matchMedia('(prefers-color-scheme: dark)');

  function stored(key) {
    try { return JSON.parse(sessionStorage.getItem(key) || '[]'); } catch { return []; }
  }

  function paintTheme(preference) {
    const isDark = preference === 'dark' || (preference === 'system' && dark.matches);
    document.documentElement.dataset.workspaceThemePreference = preference;
    document.documentElement.dataset.workspaceTheme = isDark ? 'dark' : 'light';
    document.querySelectorAll('button[data-workspace-theme]').forEach(button => {
      button.setAttribute('aria-pressed', String(isDark));
      button.title = isDark ? 'Светлая тема' : 'Тёмная тема';
    });
  }

  const compactTools = matchMedia('(max-width: 650px)');
  function syncAccountMenu() {
    const panel = document.getElementById('workspace-account-menu');
    const toggle = document.querySelector('.workspace-account-toggle');
    if (!toggle || !panel || typeof panel.showPopover !== 'function') return;
    const hadFocus = panel.contains(document.activeElement) || toggle === document.activeElement;
    if (compactTools.matches) {
      if (panel.hasAttribute('popover')) return;
      panel.setAttribute('popover', 'auto');
      toggle.hidden = false;
      if (hadFocus) toggle.focus({preventScroll:true});
    } else {
      if (!panel.hasAttribute('popover')) return;
      panel.removeAttribute('popover');
      toggle.hidden = true;
      if (hadFocus) panel.querySelector('button')?.focus({preventScroll:true});
    }
  }
  compactTools.addEventListener('change', syncAccountMenu);
  syncAccountMenu();
  document.addEventListener('toggle', event => {
    if (event.target.id !== 'workspace-account-menu' || event.newState !== 'closed') return;
    const notices = event.target.querySelector('details[open]');
    if (notices) notices.open = false;
  }, true);

  function openSearch(target) {
    const existing = document.querySelector('#workspace-quick-search');
    if (existing?.open) return;
    const previous = document.activeElement;
    const dialog = document.createElement('dialog');
    dialog.id = 'workspace-quick-search';
    dialog.className = 'workspace-quick-search';
    dialog.setAttribute('aria-labelledby','workspace-quick-title');
    dialog.innerHTML = `
      <form method="get">
        <header><h2 id="workspace-quick-title">Поиск</h2>
          <button type="button" class="parity-icon" aria-label="Закрыть поиск">×</button></header>
        <label for="workspace-quick-query">Название, текст или ID задачи</label>
        <input id="workspace-quick-query" type="search" name="q" maxlength="100" autocomplete="off">
        <button type="submit" class="space-action primary-action">Найти</button>
      </form>`;
    dialog.querySelector('form').action = target.href;
    dialog.querySelector('header button').onclick = () => dialog.close();
    dialog.addEventListener('keydown', event => {
      if (event.key !== 'Escape' || event.isComposing) return;
      event.preventDefault();
      event.stopPropagation();
      dialog.close();
    });
    dialog.addEventListener('close', () => {
      dialog.remove();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    }, {once: true});
    document.body.append(dialog);
    dialog.showModal();
    dialog.querySelector('input').focus();
  }

  let theme = 'dark';
  try {
    const saved = localStorage.getItem(themeKey);
    if (['light', 'dark', 'system'].includes(saved)) theme = saved;
    sessionStorage.removeItem('sever:workspace-recent');
  } catch {}
  paintTheme(theme);
  dark.addEventListener('change', () => paintTheme(theme));
  document.addEventListener('sever:workspace-theme', event => {
    if (!['light', 'dark', 'system'].includes(event.detail)) return;
    theme = event.detail;
    paintTheme(theme);
    try { localStorage.setItem(themeKey, theme); } catch {}
  });

  function recentIds(key) {
    const values = stored(key);
    return Array.isArray(values) ? values.filter(value => typeof value === 'string' && validId.test(value)).slice(0, 12) : [];
  }

  function rememberPage(header) {
    const match = pagePath.exec(location.pathname);
    const owner = header.dataset.recentOwner;
    if (!match || match[1] !== header.dataset.workspaceTenant || !validId.test(owner || '')) return;
    const key = recentPrefix + owner + ':' + match[1];
    const id = match[3];
    try { sessionStorage.setItem(key, JSON.stringify([id, ...recentIds(key).filter(value => value !== id)].slice(0, 12))); } catch {}
  }

  async function readJson(path, signal) {
    const response = await fetch(path, {headers: {Accept: 'application/json'}, cache: 'no-store', redirect: 'error', signal});
    if (!response.ok) throw Error(String(response.status));
    return response.json();
  }

  function row(label, href) {
    const item = document.createElement('li');
    if (href) {
      const link = document.createElement('a');
      link.href = href;
      link.textContent = label;
      item.append(link);
    } else {
      item.textContent = label;
    }
    return item;
  }

  const pending = new WeakMap();
  async function fetchPanel(panel, load, unavailable) {
    pending.get(panel)?.abort();
    const controller = new AbortController();
    pending.set(panel, controller);
    try {
      await load(controller.signal);
    } catch {
      if (!controller.signal.aborted) panel.querySelector('ul').replaceChildren(row(unavailable));
    }
  }

  function showRecent(panel, header, base, tenant) {
    const list = panel.querySelector('ul');
    list.replaceChildren(row('Загрузка…'));
    return fetchPanel(panel, async signal => {
      const owner = header.dataset.recentOwner ?? (await readJson(base + '/chrome', signal)).accountId;
      const ids = recentIds(recentPrefix + owner + ':' + tenant);
      const data = ids.length ? await readJson(base + '/recent?' + new URLSearchParams(ids.map(id => ['id',id])), signal) : {items: []};
      if (!panel.open || signal.aborted) return;
      const entries = data.items.filter(item => pagePath.exec(item.path)?.[1] === tenant);
      list.replaceChildren(...(entries.length ? entries.map(item => row(item.label, item.path)) :
        [row('Пока нет недавних документов')]));
    }, 'Не удалось загрузить недавние документы');
  }

  function tenantRow(base, tenant, item) {
    if (item.id === tenant) {
      const current = row(item.name, base + '/organization');
      current.firstChild.setAttribute('aria-current', 'true');
      return current;
    }
    const li = document.createElement('li');
    const form = document.createElement('form');
    const button = document.createElement('button');
    form.className = 'parity-switch';
    form.method = 'post';
    form.action = '/workspace/' + item.id + '/organization/switch';
    button.type = 'submit';
    button.textContent = item.name;
    form.append(button);
    li.append(form);
    return li;
  }

  function showPicker(panel, header, base, tenant) {
    const list = panel.querySelector('ul');
    const type = panel.dataset.nativePicker;
    const space = header.dataset.workspaceSpace;
    const input = panel.querySelector('input');
    list.replaceChildren(row('Загрузка…'));
    input?.focus();
    return fetchPanel(panel, async signal => {
      const data = await readJson(base + '/chrome', signal);
      if (!panel.open || signal.aborted) return;
      if (type === 'tenants') {
        list.replaceChildren(...data.tenants.map(item => tenantRow(base, tenant, item)));
      } else if (type === 'spaces') {
        const render = () => {
          const search = input.value.trim().normalize('NFC').toLowerCase();
          const results = data.spaces.filter(item => item.name.normalize('NFC').toLowerCase().includes(search));
          list.replaceChildren(...(results.length ? results.map(item => {
            const li = row(item.name, base + '/spaces/' + item.id);
            if (item.id === space) li.firstChild.setAttribute('aria-current','page');
            return li;
          }) : [row('Пространства не найдены')]));
        };
        input.oninput = render;
        render();
      } else {
        const match = data.spaces.find(item => item.id === space);
        const path = base + '/spaces/' + space;
        const items = [];
        if (match?.canCreateSection) items.push(row('Раздел', path + '/sections/new'));
        if (match?.isOwner) items.push(row('Задача', path + '/tasks/new'));
        list.replaceChildren(...(items.length ? items : [row('У вас нет прав на создание')]));
      }
    }, 'Разделы меню недоступны');
  }

  const noticePath = new RegExp(pagePath.source.slice(0,-1) +
    '(?:[?]thread=' + uuid + '(?:&comment=' + uuid + ')?#comment-' + uuid + ')?$');
  function showNotifications(panel, offset = 0) {
    const list = panel.querySelector('ul');
    const pager = panel.querySelector('.collaboration-pager');
    if (offset || !list.childElementCount) list.replaceChildren(row('Загрузка…'));
    return fetchPanel(panel, async signal => {
      const data = await readJson('/workspace/notifications?offset=' + offset, signal);
      if (!panel.open || signal.aborted) return;
      const items = data.notifications.filter(item => noticePath.test(item.href));
      list.replaceChildren(...(items.length ? items.map(item => row(
        item.kind === 'publication' ? item.title + ' · Редакция ' + item.revision : item.author + ' упоминает вас в комментарии',
        item.href
      )) : [row('Уведомлений пока нет')]));
      const control = (label, next) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.textContent = label;
        button.className = 'space-action space-secondary';
        button.onclick = () => showNotifications(panel, next);
        return button;
      };
      const buttons = [];
      if (offset) buttons.push(control('С начала', 0));
      if (Number.isSafeInteger(data.nextOffset)) buttons.push(control('Дальше', data.nextOffset));
      pager.replaceChildren(...buttons);
    }, 'Не удалось загрузить уведомления');
  }

  const menus = '[data-workspace-recent], [data-native-picker], [data-native-notifications]';
  document.addEventListener('toggle', event => {
    const panel = event.target;
    if (!(panel instanceof HTMLDetailsElement) || !panel.matches(menus)) return;
    if (!panel.open) {
      pending.get(panel)?.abort();
      panel.querySelector('ul').replaceChildren();
      if (panel.dataset.nativePicker === 'spaces') {
        const input = panel.querySelector('input');
        input.oninput = null;
        input.value = '';
      }
      return;
    }
    const header = panel.closest('[data-workspace-tenant]');
    if (!header) return;
    const tenant = header.dataset.workspaceTenant;
    const base = '/workspace/' + tenant;
    if (panel.hasAttribute('data-workspace-recent')) void showRecent(panel, header, base, tenant);
    else if (panel.hasAttribute('data-native-notifications')) void showNotifications(panel);
    else void showPicker(panel, header, base, tenant);
  }, true);

  document.addEventListener('click', event => {
    if (!(event.target instanceof Element)) return;
    document.querySelectorAll(menus).forEach(panel => {
      if (panel.open && !panel.contains(event.target)) panel.open = false;
    });
    const button = event.target.closest('button[data-workspace-theme], button[data-workspace-history]');
    if (!button) return;
    if (button.hasAttribute('data-workspace-theme')) {
      const next = document.documentElement.dataset.workspaceTheme === 'dark' ? 'light' : 'dark';
      document.dispatchEvent(new CustomEvent('sever:workspace-theme', {detail: next}));
    } else if (button.dataset.workspaceHistory === 'back') history.back();
    else history.forward();
  });

  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && event.target instanceof Element) {
      const panel = event.target.closest('details[open]');
      if (panel?.matches(menus) || panel?.closest('#workspace-account-menu[popover]')) {
        panel.open = false;
        panel.querySelector('summary').focus();
        event.preventDefault();
      }
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
      const target = document.querySelector('[data-workspace-search]');
      if (target instanceof HTMLAnchorElement) {
        event.preventDefault();
        openSearch(target);
      }
    }
  });

  const root = document.querySelector('#sever-root');
  if (root) {
    const syncReactHeader = () => {
      syncAccountMenu();
      const header = root.querySelector('[data-recent-owner]');
      if (header) rememberPage(header);
      paintTheme(theme);
    };
    if (root.dataset.renderer === 'react') syncReactHeader();
    else {
      const observer = new MutationObserver(() => {
        observer.disconnect();
        syncReactHeader();
      });
      observer.observe(root, {attributes: true, attributeFilter: ['data-renderer']});
    }
  }

  const native = document.querySelector('[data-workspace-space]');
  if (native) {
    const {workspaceTenant, workspaceSpace} = native.dataset;
    const base = '/workspace/' + workspaceTenant;
    readJson(base + '/chrome').then(data => {
      const space = data.spaces.find(item => item.id === workspaceSpace);
      const access = native.querySelector('[data-native-space-access]');
      if (space?.canManageAccess && access) {
        access.href = base + '/spaces/' + workspaceSpace + '/access';
        access.hidden = false;
      }
      const create = native.querySelector('[data-native-picker="create"]');
      if (create) create.hidden = !space?.canCreateSection && !space?.isOwner;
    }).catch(() => {});
  }
  window.addEventListener('pageshow', event => {
    if (event.persisted) document.querySelectorAll(menus).forEach(panel => { panel.open = false; });
  });
})();
