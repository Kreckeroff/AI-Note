import {emailMaxLength, emailPattern, nameMaxLength, passwordMaxLength, passwordMinLength} from './identity.mjs';
import {registrationConfirmationPath, registrationPath} from './registration-paths.mjs';
import {spacesPath} from './workspace-paths.mjs';
import {escapeHtml, northIcon, page} from './page-view.mjs';

function accountPage(title, content, withForm = false) {
  return page(title, `
    <main class="auth-screen">
      <section class="auth-card" aria-labelledby="page-title">
        <div class="brand"><span class="brand-mark">${northIcon}</span>Север</div>
        ${content}
      </section>
    </main>`, {script: withForm ? '/sign-in.mjs' : undefined});
}

export function signInPage({email = '', error = '', registrationEnabled = false, returnTo = ''} = {}) {
  const message = error ? `<p class="form-error" id="sign-in-error" role="alert" tabindex="-1" autofocus>${escapeHtml(error)}</p>` : '';
  return accountPage('Вход в аккаунт', `
        <header class="auth-heading">
          <h1 id="page-title">Вход в аккаунт</h1>
          <p>${registrationEnabled ? 'Введите почту и пароль от аккаунта.' : 'Введите почту и пароль, которые выдал администратор.'}</p>
        </header>
        <form method="post" action="/login"${error ? ' aria-describedby="sign-in-error"' : ''}>
          ${message}
          ${returnTo ? `<input type="hidden" name="returnTo" value="${escapeHtml(returnTo)}">` : ''}
          <label for="email">Рабочая почта</label>
          <input id="email" type="email" name="email" value="${escapeHtml(email)}" autocomplete="username" required maxlength="${emailMaxLength}" pattern="${escapeHtml(emailPattern)}" title="Укажите рабочую почту, например editor@company.ru" spellcheck="false" autocapitalize="none">
          <label for="password">Пароль</label>
          <input id="password" type="password" name="password" autocomplete="current-password" required maxlength="256">
          <button class="primary-action" type="submit">Войти</button>
        </form>
        <p class="sr-only" role="status" data-sign-in-status></p>
        ${registrationEnabled ? `<p><a class="auth-link" href="${registrationPath}">Зарегистрироваться</a></p>` : ''}`, true);
}

function registrationError(error) {
  return error ? `<p class="form-error" id="registration-error" role="alert" tabindex="-1" autofocus>${escapeHtml(error)}</p>` : '';
}

export function registrationStartPage({email = '', error = ''} = {}) {
  return accountPage('Регистрация', `
    <header class="auth-heading">
      <h1 id="page-title">Регистрация</h1>
      <p>Укажите почту, чтобы получить ссылку для подтверждения.</p>
    </header>
    <form method="post" action="${registrationPath}"${error ? ' aria-describedby="registration-error"' : ''}>
      ${registrationError(error)}
      <label for="email">Почта</label>
      <input id="email" type="email" name="email" value="${escapeHtml(email)}" autocomplete="email" required maxlength="${emailMaxLength}" pattern="${escapeHtml(emailPattern)}" spellcheck="false" autocapitalize="none">
      <button class="primary-action" type="submit">Отправить ссылку</button>
    </form>
    <p><a class="auth-link" href="/">Вернуться ко входу</a></p>`);
}

export function registrationRequestedPage() {
  return accountPage('Проверьте почту', `
    <header class="auth-heading">
      <h1 id="page-title">Проверьте почту</h1>
      <p>Если адрес доступен для регистрации, на него придёт ссылка. Откройте её на компьютере, где запущен «Север».</p>
    </header>
    <p><a class="auth-link" href="${registrationPath}">Запросить ссылку ещё раз</a></p>
    <p><a class="auth-link" href="/">Вернуться ко входу</a></p>`);
}

export function registrationCompletePage({token, name = '', error = ''}) {
  return accountPage('Завершите регистрацию', `
    <header class="auth-heading">
      <h1 id="page-title">Завершите регистрацию</h1>
      <p>Укажите имя и придумайте пароль для своей личной среды.</p>
    </header>
    <form method="post" action="${registrationConfirmationPath}"${error ? ' aria-describedby="registration-error"' : ''}>
      ${registrationError(error)}
      <input type="hidden" name="token" value="${escapeHtml(token)}">
      <label for="name">Имя</label>
      <input id="name" name="name" value="${escapeHtml(name.slice(0, nameMaxLength * 2))}" autocomplete="name" required maxlength="${nameMaxLength * 2}">
      <label for="password">Пароль</label>
      <p class="auth-hint" id="registration-password-help">От ${passwordMinLength} до ${passwordMaxLength} символов.</p>
      <input id="password" type="password" name="password" autocomplete="new-password" aria-describedby="registration-password-help" required minlength="${passwordMinLength}" maxlength="${passwordMaxLength * 2}">
      <button class="primary-action" type="submit">Создать аккаунт</button>
    </form>
    <p><a class="auth-link" href="/">Вернуться ко входу</a></p>`);
}

export function workspacePage(name, tenants = []) {
  const alternatives = tenants.map(tenant => `<li><a class="auth-link" href="${spacesPath(tenant.id)}">${escapeHtml(tenant.name)}</a></li>`).join('');
  return accountPage('Рабочая область', `
    <header class="auth-heading">
      <h1 id="page-title">Нет доступной среды</h1>
      <p>Вы вошли как <strong>${escapeHtml(name)}</strong>.</p>
    </header>
    ${alternatives ? `<ul>${alternatives}</ul>` : ''}`);
}
