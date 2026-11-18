import {emailMaxLength, emailPattern} from './identity.mjs';

const entities = {'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'};
const northIcon = `<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2l2.8 7.2L22 12l-7.2 2.8L12 22l-2.8-7.2L2 12l7.2-2.8L12 2z M12 9v6m-3-3h6"/></svg>`;

function escapeHtml(value) {
  return value.replace(/[&<>"']/gu, character => entities[character]);
}

function accountPage(title, content, withForm = false) {
  return `<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>${title} · Север</title>
    <link rel="stylesheet" href="/auth.css">
    ${withForm ? '<script type="module" src="/sign-in.mjs"></script>' : ''}
  </head>
  <body>
    <main class="auth-screen">
      <section class="auth-card" aria-labelledby="page-title">
        <div class="brand"><span class="brand-mark">${northIcon}</span>Север</div>
        ${content}
      </section>
    </main>
  </body>
</html>`;
}

export function signInPage({email = '', error = ''} = {}) {
  const message = error ? `<p class="form-error" id="sign-in-error" role="alert" tabindex="-1" autofocus>${escapeHtml(error)}</p>` : '';
  return accountPage('Вход в аккаунт', `
        <header class="auth-heading">
          <h1 id="page-title">Вход в аккаунт</h1>
          <p>Введите почту и пароль, которые выдал администратор.</p>
        </header>
        <form method="post" action="/login"${error ? ' aria-describedby="sign-in-error"' : ''}>
          ${message}
          <label for="email">Рабочая почта</label>
          <input id="email" type="email" name="email" value="${escapeHtml(email)}" autocomplete="username" required maxlength="${emailMaxLength}" pattern="${escapeHtml(emailPattern)}" title="Укажите рабочую почту, например editor@company.ru" spellcheck="false" autocapitalize="none">
          <label for="password">Пароль</label>
          <input id="password" type="password" name="password" autocomplete="current-password" required maxlength="256">
          <button class="primary-action" type="submit">Войти</button>
        </form>
        <p class="sr-only" role="status" data-sign-in-status></p>`, true);
}

export function workspacePage(name) {
  return accountPage('Рабочая область', `
        <header class="auth-heading">
          <h1 id="page-title">Рабочая область</h1>
          <p>Вы вошли как <strong>${escapeHtml(name)}</strong>.</p>
        </header>`);
}
