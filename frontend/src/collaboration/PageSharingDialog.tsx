import {useId, useLayoutEffect, useRef, useState, type FormEvent} from 'react';
import {Icon} from '../Icon';
import {externalPageEmbedHint, teamStreamPublicHint} from '../editor/pageEmbedInput';
import {queryTo, roleLabels, useCollaborationQuery, usePageCommand, type PageRole} from './api';
import {CollaborationTabs} from './CollaborationTabs';

const tabs = [['people', 'Люди'], ['link', 'Ссылка'], ['requests', 'Запросы']] as const;
type SharingTab = typeof tabs[number][0];
type Person = {
  id: string;
  name: string;
  email: string;
  direct: PageRole | null;
  inherited: PageRole | null;
  effective: PageRole | null;
  source: 'owner' | 'space' | 'tenant' | null;
  guest: boolean;
};
type AccessRequest = {id: string; name: string; status: 'pending' | 'approved' | 'denied'};
type PeoplePage = {people: Person[]; requests: AccessRequest[]; nextOffset: number | null};
type Invites = {
  allowed: boolean;
  deliveryConfigured: boolean;
  nextOffset: number | null;
  invites: {id: string; email: string; role: PageRole; status: 'pending' | 'accepted' | 'revoked' | 'expired'; delivery: 'pending' | 'sent'}[];
};
type LinkSettings = {
  link: {id: string; href: string | null} | null;
  sharingConfigured: boolean;
  containsEmbed?: boolean;
  containsTeamStream?: boolean;
  policy: {allowed: boolean; enabled: boolean; version: number; affected: {id: string; title: string}[]};
};
type ShareCommand = ReturnType<typeof usePageCommand>;

const sourceLabels = {owner: 'владение пространством', space: 'право пространства', tenant: 'роль организации'};
const requestLabels = {pending: 'Запрошен просмотр', approved: 'Просмотр выдан', denied: 'Отклонён'};
const inviteLabels = {pending: 'Ожидает принятия', accepted: 'Принято', revoked: 'Отменено', expired: 'Истекло'};

function RoleSelect({name, value, label, onChange, disabled}: {
  name?: string;
  label: string;
  value: PageRole | '';
  onChange(value: PageRole): void;
  disabled?: boolean;
}) {
  return <select name={name} aria-label={label} value={value} disabled={disabled}
    onChange={event => onChange(event.currentTarget.value as PageRole)}>
    {value === '' && <option value="" disabled>Выдать доступ</option>}
    <option value="read">Просмотр</option>
    <option value="comment">Комментирование</option>
    <option value="edit">Редактирование</option>
  </select>;
}

function Pagination({offset, nextOffset, previous, next}: {
  offset: number;
  nextOffset?: number | null;
  previous(): void;
  next(offset: number): void;
}) {
  if (!offset && nextOffset == null) return null;
  return <div className="collaboration-pager">
    <button type="button" className="space-action space-secondary" onClick={previous} disabled={!offset}>Назад</button>
    <button type="button" className="space-action space-secondary" onClick={() => next(nextOffset!)}
      disabled={nextOffset == null}>Дальше</button>
  </div>;
}

function ConfirmAction({heading, description, label, command, onConfirm, onCancel}: {
  heading: string;
  description: React.ReactNode;
  label?: string;
  command: ShareCommand;
  onConfirm?(): void;
  onCancel(): void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  useLayoutEffect(() => { cancel.current?.focus(); }, []);
  return <section className="sharing-confirmation" aria-label={heading}>
    <h3>{heading}</h3>
    {description}
    <div className="space-dialog-actions">
      <button ref={cancel} type="button" className="space-action space-secondary" onClick={onCancel}>Отмена</button>
      {label && onConfirm && <button type="button" className="space-action primary-action"
        disabled={command.busy} onClick={onConfirm}>{label}</button>}
    </div>
  </section>;
}

function GuestInvitations({invites, command}: {invites?: Invites; command: ShareCommand}) {
  const [email, setEmail] = useState('');
  const [role, setRole] = useState<PageRole>('read');

  async function invite(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (await command.send({action: 'invite', email: email.trim(), role})) setEmail('');
  }

  if (!invites) return <p role="status">Загружаем приглашения…</p>;
  const canInvite = invites.allowed && invites.deliveryConfigured;
  return <>
    {!invites.allowed && <p>Политика организации запрещает приглашать гостей.</p>}
    {!invites.deliveryConfigured && <p>Почта для приглашений не настроена. Обратитесь к администратору.</p>}
    {canInvite && <form className="sharing-invite" onSubmit={invite}>
      <label>Почта гостя<input type="email" required maxLength={254} value={email}
        placeholder="name@example.com" onChange={event => setEmail(event.currentTarget.value)} /></label>
      <label>Роль<RoleSelect label="Роль гостя" value={role} onChange={setRole} disabled={command.busy} /></label>
      <button className="space-action primary-action" type="submit" disabled={command.busy}>Пригласить</button>
    </form>}
    {invites.invites.length > 0 && <ul className="sharing-people">
      {invites.invites.map(item => <li key={item.id}>
        <div className="sharing-person">
          <strong>{item.email}</strong>
          <small>{roleLabels[item.role]} · {inviteLabels[item.status]}</small>
          {item.status === 'pending' && item.delivery === 'pending' && <small>Письмо не отправлено</small>}
        </div>
        {canInvite && item.status === 'pending' && item.delivery === 'pending' &&
          <button type="button" className="space-action space-secondary" disabled={command.busy}
            onClick={() => void command.send({action: 'invite', email: item.email, role: item.role})}>Повторить отправку</button>}
        {item.status === 'pending' && <button type="button" className="space-action space-secondary"
          disabled={command.busy} onClick={() => void command.send({action: 'cancelInvite', inviteId: item.id})}>
          Отменить
        </button>}
        {canInvite && (item.status === 'expired' || item.status === 'revoked') &&
          <button type="button" className="space-action space-secondary" disabled={command.busy}
            onClick={() => void command.send({action: 'invite', email: item.email, role: item.role})}>Пригласить снова</button>}
      </li>)}
    </ul>}
  </>;
}

type LinkConfirmation = {kind: 'policy'; version: number} | {kind: 'unpublish'; linkId: string};

function SharingLinks({pageTo, settings, command, onPublicLinkChange}: {
  pageTo: string; settings?: LinkSettings; command: ShareCommand; onPublicLinkChange?(active: boolean): void;
}) {
  const [confirmation, setConfirmation] = useState<LinkConfirmation | null>(null);
  const [notice, setNotice] = useState('');
  if (!settings) {
    return confirmation && <ConfirmAction
      heading={confirmation.kind === 'policy' ? 'Запретить публичные ссылки?' : 'Отключить публичную ссылку?'}
      command={command} onCancel={() => setConfirmation(null)}
      description={<p>Подтверждение станет доступно после загрузки настроек.</p>} />;
  }
  const {link, policy, sharingConfigured} = settings;
  const ordinary = new URL(pageTo, window.location.origin).href;
  // Published URLs always refer to this application. Only render a validated href.
  const publicHref = (() => {
    try {
      if (!link?.href) return null;
      const url = new URL(link.href, window.location.origin);
      return url.origin === window.location.origin && /^\/published\/[0-9a-f-]{36}\/[0-9A-Za-z_-]{16,128}$/u.test(url.pathname)
        && !url.search && !url.hash ? url.href : null;
    } catch { return null; }
  })();

  async function copy(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setNotice('Ссылка скопирована.');
    } catch {
      setNotice('Выделите и скопируйте ссылку из поля.');
    }
  }

  if (confirmation?.kind === 'policy') {
    const affected = policy.affected;
    return <ConfirmAction key={policy.version} heading="Запретить публичные ссылки?" command={command}
      label={policy.enabled ? 'Запретить' : undefined}
      onCancel={() => setConfirmation(null)} onConfirm={async () => {
        const action = {action: 'setPublicPolicy', enabled: false, version: policy.version};
        if (await command.send(action)) {
          onPublicLinkChange?.(false);
          setConfirmation(null);
        }
      }}
      description={<>
        {confirmation.version !== policy.version && policy.enabled &&
          <p role="status">Список изменился. Проверьте затронутые страницы и подтвердите запрет.</p>}
        {!policy.enabled ? <p>Публичные ссылки уже запрещены.</p> : affected.length > 0
          ? <>
            <p>Ссылки будут отключены на всех страницах в списке ({affected.length}).</p>
            <ul className="sharing-affected">{affected.map(page => <li key={page.id}>{page.title}</li>)}</ul>
          </>
          : <p>Сейчас в пространстве нет действующих публичных ссылок.</p>}
      </>} />;
  }
  if (confirmation?.kind === 'unpublish') {
    const linkId = link?.id === confirmation.linkId ? link.id : undefined;
    return <ConfirmAction heading="Отключить публичную ссылку?" command={command}
      label={linkId ? 'Отключить' : undefined}
      onCancel={() => setConfirmation(null)} onConfirm={async () => {
        if (linkId && await command.send({action: 'unpublish', linkId})) {
          onPublicLinkChange?.(false);
          setConfirmation(null);
        }
      }}
      description={<p>{linkId ? 'Публичный адрес страницы и её изображений перестанет работать.'
        : 'Эта публичная ссылка уже отключена.'}</p>} />;
  }
  return <>
    <section className="sharing-link-box">
      <h3>Обычная ссылка</h3>
      <p>Откроется у пользователей с доступом к странице.</p>
      <input aria-label="Обычная ссылка" readOnly value={ordinary} onFocus={event => event.currentTarget.select()} />
      <button type="button" className="space-action space-secondary" onClick={() => void copy(ordinary)}>Скопировать</button>
    </section>
    <section className="sharing-link-box">
      <h3>Публичная ссылка</h3>
      {!sharingConfigured && <p>Публичные адреса не настроены. Обратитесь к администратору.</p>}
      <p>{link ? 'Открывает текущую страницу, обложку и встроенные изображения.'
        : policy.enabled ? 'Публичная ссылка ещё не создана.' : 'Запрещена политикой пространства.'}</p>
      {settings.containsEmbed === true && (link || sharingConfigured && policy.allowed && policy.enabled) &&
        <p className="collaboration-hint">{externalPageEmbedHint}{settings.containsTeamStream === true && ` ${teamStreamPublicHint}`}</p>}
      {link ? <>
        {publicHref && <input aria-label="Публичная ссылка" readOnly value={publicHref}
          onFocus={event => event.currentTarget.select()} />}
        <div className="sharing-actions">
          {publicHref && <>
            <button type="button" className="space-action space-secondary"
              onClick={() => void copy(publicHref)}>Скопировать</button>
            <a className="space-action space-secondary" href={publicHref} target="_blank" rel="noreferrer">Посмотреть</a>
          </>}
          <button type="button" className="space-action space-secondary"
            onClick={() => setConfirmation({kind: 'unpublish', linkId: link.id})}>Отключить публичную ссылку</button>
        </div>
      </> : <button type="button" className="space-action primary-action" disabled={!sharingConfigured || !policy.allowed || !policy.enabled || command.busy}
        onClick={async () => { if (await command.send({action: 'publish'})) onPublicLinkChange?.(true); }}>Создать публичную ссылку</button>}
    </section>
    <details className="sharing-link-box sharing-policy">
      <summary>Политика пространства</summary>
      <p>{!policy.allowed ? 'Политика организации запрещает публичные ссылки.'
        : policy.enabled ? 'В этом пространстве можно создавать публичные ссылки.' : 'Публичные ссылки запрещены.'}</p>
      <button type="button" className="space-action space-secondary" disabled={command.busy || !policy.enabled && (!sharingConfigured || !policy.allowed)}
        onClick={() => policy.enabled ? setConfirmation({kind: 'policy', version: policy.version})
          : void command.send({action: 'setPublicPolicy', enabled: true, version: policy.version})}>
        {policy.enabled ? 'Запретить публичные ссылки' : 'Разрешить публичные ссылки'}
      </button>
    </details>
    <p role="status" className="collaboration-hint">{notice}</p>
  </>;
}

export function PageSharingDialog({pageTo, title, onDismiss, onPublicLinkChange}: {
  pageTo: string; title: string; onDismiss(): void; onPublicLinkChange?(active: boolean): void;
}) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const [tab, setTab] = useState<SharingTab>('people');
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const [offsets, setOffsets] = useState([0]);
  const [refresh, setRefresh] = useState(0);
  const [inviting, setInviting] = useState(false);
  const [guestOffsets, setGuestOffsets] = useState([0]);
  const [revoke, setRevoke] = useState<Person | null>(null);
  const action = `${pageTo}/sharing`;
  const offset = offsets.at(-1)!;
  const listing = useCollaborationQuery<PeoplePage>(
    tab !== 'link' ? queryTo(action, {tab, q: tab === 'people' ? query : '', filter, offset}) : null, refresh,
  );
  const link = useCollaborationQuery<LinkSettings>(tab === 'link' ? queryTo(action, {tab}) : null, refresh);
  const guestOffset = guestOffsets.at(-1)!;
  const guest = useCollaborationQuery<Invites>(tab === 'people' && inviting
    ? queryTo(action, {tab: 'invites', offset: guestOffset}) : null, refresh);
  const reload = () => setRefresh(value => value + 1);
  const command = usePageCommand(action, reload, reload);

  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    close.current?.focus({preventScroll: true});
    return () => element?.close();
  }, []);

  const current = tab === 'link' ? link : listing;
  const pagination = <Pagination offset={offset} nextOffset={listing?.data?.nextOffset}
    previous={() => setOffsets(value => value.slice(0, -1))}
    next={next => setOffsets(value => [...value, next])} />;

  function switchTab(value: SharingTab) {
    setOffsets([0]);
    setRevoke(null);
    command.clearError();
    setTab(value);
  }

  return <dialog className="space-dialog page-sharing-dialog" ref={dialog} aria-labelledby={`${id}-title`}
    onCancel={event => { event.preventDefault(); onDismiss(); }}>
    <header className="space-dialog-header">
      <div><h2 id={`${id}-title`}>Поделиться страницей</h2><p className="space-dialog-tenant">{title}</p></div>
      <button className="space-action space-close" ref={close} type="button" aria-label="Закрыть окно доступа"
        onClick={onDismiss}><Icon name="close" /></button>
    </header>
    <div className="page-sharing-body">
      <CollaborationTabs id={id} label="Управление доступом" tabs={tabs} value={tab} onChange={switchTab} />
      {command.error && <p className="form-error" role="alert">{command.error}</p>}
      {!current && <p role="status" className="collaboration-hint">Загружаем настройки доступа…</p>}
      {current?.error && <p className="form-error" role="alert">{current.error} <button type="button"
        className="space-action space-secondary" onClick={() => setRefresh(value => value + 1)}>Повторить</button></p>}
      {tabs.filter(([key]) => key !== tab).map(([key]) => <div key={key} hidden role="tabpanel"
        id={`${id}-panel-${key}`} aria-labelledby={`${id}-tab-${key}`} />)}
      <section className="page-sharing-tab" role="tabpanel" id={`${id}-panel-${tab}`} aria-labelledby={`${id}-tab-${tab}`}>
        {tab === 'people' && (revoke ? <ConfirmAction heading="Отозвать прямой доступ?" command={command}
          label="Отозвать" onCancel={() => setRevoke(null)} onConfirm={async () => {
            if (await command.send({action: 'revoke', accountId: revoke.id})) setRevoke(null);
          }}
          description={<p>{revoke.name}. {revoke.inherited
            ? `Сохранится роль «${roleLabels[revoke.inherited]}» через ${sourceLabels[revoke.source ?? 'space']}.`
            : 'Адресный доступ к странице будет закрыт.'}</p>} /> : <>
          <div className="sharing-actions">
            <input type="search" aria-label="Поиск по имени или почте" placeholder="Поиск по имени или почте"
              value={query} maxLength={180} onChange={event => { setQuery(event.currentTarget.value); setOffsets([0]); }} />
            <select aria-label="Источник доступа" value={filter}
              onChange={event => { setFilter(event.currentTarget.value); setOffsets([0]); }}>
              <option value="all">Все участники</option><option value="direct">Прямой доступ</option>
              <option value="inherited">Унаследованный</option><option value="available">Без доступа</option>
            </select>
          </div>
          {listing?.data && (listing.data.people.length ? <ul className="sharing-people">
            {listing.data.people.map(person => <li key={person.id}>
              <div className="sharing-person"><strong>{person.name}</strong><small>{person.email}</small>
                <small>{person.guest ? 'Гость · ' : ''}{person.source === 'owner' ? roleLabels.owner
                  : person.effective ? `Итог · ${roleLabels[person.effective]}` : 'Доступа нет'}</small>
                {person.inherited && person.source !== 'owner' && <small>
                  {roleLabels[person.inherited]} · {sourceLabels[person.source ?? 'space']}
                </small>}
              </div>
              {(!person.guest || person.direct) && person.source !== 'owner' && <RoleSelect label={`Прямой доступ · ${person.name}`}
                value={person.direct ?? ''} disabled={command.busy} onChange={role => {
                  void command.send({action: 'grant', accountId: person.id, role});
                }} />}
              {person.direct && <button type="button" className="space-action space-secondary" disabled={command.busy}
                aria-label={`Отозвать прямой доступ · ${person.name}`} onClick={() => {
                  command.clearError(); setRevoke(person);
                }}>Отозвать</button>}
            </li>)}
          </ul> : <p className="collaboration-hint">Подходящих участников нет.</p>)}
          {pagination}
          <section className="sharing-guests" aria-label="Приглашения гостей">
            <button className="space-action space-secondary" type="button" aria-expanded={inviting}
              onClick={() => { setInviting(value => !value); setGuestOffsets([0]); }}>Приглашения гостей</button>
            {inviting && <>
              {guest?.error ? <p className="form-error" role="alert">{guest.error}{' '}
                <button type="button" className="space-action space-secondary" onClick={reload}>Повторить</button></p>
                : <GuestInvitations invites={guest?.data} command={command} />}
              <Pagination offset={guestOffset} nextOffset={guest?.data?.nextOffset}
                previous={() => setGuestOffsets(value => value.slice(0, -1))}
                next={value => setGuestOffsets(offsets => [...offsets, value])} />
            </>}
          </section>
        </>)}
        {tab === 'link' && <SharingLinks pageTo={pageTo} settings={link?.data} command={command} onPublicLinkChange={onPublicLinkChange} />}
        {tab === 'requests' && listing?.data && <>
          {listing.data.requests.length ? <ul className="sharing-people">{listing.data.requests.map(item => <li key={item.id}>
            <div className="sharing-person"><strong>{item.name}</strong><small>{requestLabels[item.status]}</small></div>
            {item.status === 'pending' && <>
              <button type="button" className="space-action primary-action" disabled={command.busy}
                onClick={() => void command.send({action: 'approve', requestId: item.id})}>Дать просмотр</button>
              <button type="button" className="space-action space-secondary" disabled={command.busy}
                onClick={() => void command.send({action: 'deny', requestId: item.id})}>Отклонить</button>
            </>}
          </li>)}</ul> : <p className="collaboration-hint">Запросов пока нет.</p>}
          {pagination}
        </>}
      </section>
    </div>
  </dialog>;
}
