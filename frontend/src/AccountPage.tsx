import {useEffect, useRef, useState, type FormEvent, type ReactNode} from 'react';

import {Icon} from './Icon';
import type {AccountPageModel} from './model';
import {spacesPath} from './routes';

const emailMaxLength = 254;
const emailMailbox = String.raw`[a-zA-Z0-9.!#$%&'*+\x2f=?^_\x60\x7b\x7c\x7d~\x2d]{1,64}`;
const emailDomainLabel = String.raw`[a-zA-Z0-9](?:[a-zA-Z0-9\x2d]{0,61}[a-zA-Z0-9])?`;
const emailPattern = `(?=.{1,${emailMaxLength}}$)${emailMailbox}@${emailDomainLabel}(?:\\.${emailDomainLabel})+`;
const passwordMinLength = 15;
const passwordMaxLength = 128;
const nameInputMaxLength = 240;
const signInErrorId = 'sign-in-error';
const registrationErrorId = 'registration-error';

function AuthHeading({title, children}: {title: string; children: ReactNode}) {
  return (
    <header className="auth-heading">
      <h1 id="page-title">{title}</h1>
      <p>{children}</p>
    </header>
  );
}

function FormError({id, message}: {id: string; message: string}) {
  const error = useRef<HTMLParagraphElement>(null);

  useEffect(() => {
    error.current?.focus();
  }, []);

  return (
    <p ref={error} className="form-error" id={id} role="alert" tabIndex={-1}>
      {message}
    </p>
  );
}

function EmailField({
  label,
  email,
  autoComplete,
  title,
}: {
  label: string;
  email: string;
  autoComplete: 'username' | 'email';
  title?: string;
}) {
  return (
    <>
      <label htmlFor="email">{label}</label>
      <input
        id="email"
        type="email"
        name="email"
        defaultValue={email}
        autoComplete={autoComplete}
        required
        maxLength={emailMaxLength}
        pattern={emailPattern}
        title={title}
        spellCheck={false}
        autoCapitalize="none"
      />
    </>
  );
}

function BackToSignIn() {
  return <p><a className="auth-link" href="/">Вернуться ко входу</a></p>;
}

function SignIn({page}: {page: Extract<AccountPageModel, {kind: 'signIn'}>}) {
  const pending = useRef(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    function resetSubmission() {
      pending.current = false;
      setSubmitting(false);
    }

    window.addEventListener('pageshow', resetSubmission);
    return () => window.removeEventListener('pageshow', resetSubmission);
  }, []);

  function submit(event: FormEvent<HTMLFormElement>) {
    if (pending.current) {
      event.preventDefault();
      return;
    }
    pending.current = true;
    setSubmitting(true);
  }

  return (
    <>
      <AuthHeading title="Вход в аккаунт">
        {page.registrationEnabled
          ? 'Введите почту и пароль от аккаунта.'
          : 'Введите почту и пароль, которые выдал администратор.'}
      </AuthHeading>
      <form
        method="post"
        action="/login"
        aria-describedby={page.error ? signInErrorId : undefined}
        aria-busy={submitting || undefined}
        onSubmit={submit}
      >
        {page.error && <FormError id={signInErrorId} message={page.error} />}
        {page.returnTo && <input type="hidden" name="returnTo" value={page.returnTo} />}
        <EmailField
          label="Рабочая почта"
          email={page.email}
          autoComplete="username"
          title="Укажите рабочую почту, например editor@company.ru"
        />
        <label htmlFor="password">Пароль</label>
        <input
          id="password"
          type="password"
          name="password"
          autoComplete="current-password"
          required
          maxLength={passwordMaxLength * 2}
        />
        <button className="primary-action" type="submit" disabled={submitting}>
          {submitting ? 'Входим…' : 'Войти'}
        </button>
      </form>
      <p className="sr-only" role="status" data-sign-in-status>
        {submitting ? 'Проверяем данные входа.' : ''}
      </p>
      {page.registrationEnabled && <p><a className="auth-link" href="/register">Зарегистрироваться</a></p>}
    </>
  );
}

function RegistrationStart({page}: {page: Extract<AccountPageModel, {kind: 'registrationStart'}>}) {
  return (
    <>
      <AuthHeading title="Регистрация">Укажите почту, чтобы получить ссылку для подтверждения.</AuthHeading>
      <form
        method="post"
        action="/register"
        aria-describedby={page.error ? registrationErrorId : undefined}
      >
        {page.error && <FormError id={registrationErrorId} message={page.error} />}
        <EmailField label="Почта" email={page.email} autoComplete="email" />
        <button className="primary-action" type="submit">Отправить ссылку</button>
      </form>
      <BackToSignIn />
    </>
  );
}

function RegistrationRequested() {
  return (
    <>
      <AuthHeading title="Проверьте почту">
        Если адрес доступен для регистрации, на него придёт ссылка. Откройте её на компьютере, где запущен «Север».
      </AuthHeading>
      <p><a className="auth-link" href="/register">Запросить ссылку ещё раз</a></p>
      <BackToSignIn />
    </>
  );
}

function RegistrationComplete({page}: {page: Extract<AccountPageModel, {kind: 'registrationComplete'}>}) {
  return (
    <>
      <AuthHeading title="Завершите регистрацию">Укажите имя и придумайте пароль для своей личной среды.</AuthHeading>
      <form
        method="post"
        action="/register/confirm"
        aria-describedby={page.error ? registrationErrorId : undefined}
      >
        {page.error && <FormError id={registrationErrorId} message={page.error} />}
        <input type="hidden" name="token" value={page.token} />
        <label htmlFor="name">Имя</label>
        <input
          id="name"
          name="name"
          defaultValue={page.name.slice(0, nameInputMaxLength)}
          autoComplete="name"
          required
          maxLength={nameInputMaxLength}
        />
        <label htmlFor="password">Пароль</label>
        <p className="auth-hint" id="registration-password-help">
          От {passwordMinLength} до {passwordMaxLength} символов.
        </p>
        <input
          id="password"
          type="password"
          name="password"
          autoComplete="new-password"
          aria-describedby="registration-password-help"
          required
          minLength={passwordMinLength}
          maxLength={passwordMaxLength * 2}
        />
        <button className="primary-action" type="submit">Создать аккаунт</button>
      </form>
      <BackToSignIn />
    </>
  );
}

function NoTenant({page}: {page: Extract<AccountPageModel, {kind: 'noTenant'}>}) {
  return (
    <>
      <AuthHeading title="Нет доступной среды">
        Вы вошли как <strong>{page.accountName}</strong>.
      </AuthHeading>
      {page.tenants.length > 0 && (
        <ul>
          {page.tenants.map(tenant => (
            <li key={tenant.id}>
              <a className="auth-link" href={spacesPath(tenant.id)}>{tenant.name}</a>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function AccountContent({page}: {page: AccountPageModel}) {
  switch (page.kind) {
    case 'signIn':
      return <SignIn page={page} />;
    case 'registrationStart':
      return <RegistrationStart page={page} />;
    case 'registrationRequested':
      return <RegistrationRequested />;
    case 'registrationComplete':
      return <RegistrationComplete page={page} />;
    case 'noTenant':
      return <NoTenant page={page} />;
  }
}

export function AccountPage({page}: {page: AccountPageModel}) {
  return (
    <main className="auth-screen">
      <section className="auth-card" aria-labelledby="page-title">
        <div className="brand"><span className="brand-mark"><Icon name="north" /></span>Север</div>
        <AccountContent page={page} />
      </section>
    </main>
  );
}
