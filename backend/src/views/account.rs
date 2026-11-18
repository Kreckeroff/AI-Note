use std::fmt::Write;

use crate::db::Tenant;
use crate::workspace::spaces_path;

use super::{ClientPage, NORTH_ICON, PageAssets, html, page};

const EMAIL_PATTERN: &str = r"(?=.{1,254}$)[a-zA-Z0-9.!#$%&'*+\x2f=?^_\x60\x7b\x7c\x7d~\x2d]{1,64}@[a-zA-Z0-9](?:[a-zA-Z0-9\x2d]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9\x2d]{0,61}[a-zA-Z0-9])?)+";

fn account_page(title: &str, content: &str, model: ClientPage<'_>) -> String {
    let sign_in = matches!(model, ClientPage::SignIn { .. });
    page(
        title,
        &format!(
            r#"
    <main class="auth-screen">
      <section class="auth-card" aria-labelledby="page-title">
        <div class="brand"><span class="brand-mark">{NORTH_ICON}</span>Север</div>
        {content}
      </section>
    </main>"#
        ),
        PageAssets::Account { sign_in },
        model,
    )
}

fn form_error(error: &str, id: &str) -> (String, String) {
    if error.is_empty() {
        return (String::new(), String::new());
    }
    (
        format!(r#" aria-describedby="{id}""#),
        format!(
            r#"<p class="form-error" id="{id}" role="alert" tabindex="-1" autofocus>{}</p>"#,
            html(error)
        ),
    )
}

pub(crate) fn sign_in(
    email: &str,
    error: &str,
    registration_enabled: bool,
    return_to: &str,
) -> String {
    let (described_by, message) = form_error(error, "sign-in-error");
    let return_field = if return_to.is_empty() {
        String::new()
    } else {
        format!(
            r#"<input type="hidden" name="returnTo" value="{}">"#,
            html(return_to)
        )
    };
    let (instructions, registration_link) = if registration_enabled {
        (
            "Введите почту и пароль от аккаунта.",
            r#"<p><a class="auth-link" href="/register">Зарегистрироваться</a></p>"#,
        )
    } else {
        ("Введите почту и пароль, которые выдал администратор.", "")
    };
    account_page(
        "Вход в аккаунт",
        &format!(
            r#"
        <header class="auth-heading">
          <h1 id="page-title">Вход в аккаунт</h1>
          <p>{instructions}</p>
        </header>
        <form method="post" action="/login"{described_by}>
          {message}
          {return_field}
          <label for="email">Рабочая почта</label>
          <input id="email" type="email" name="email" value="{email}" autocomplete="username" required maxlength="254" pattern="{email_pattern}" title="Укажите рабочую почту, например editor@company.ru" spellcheck="false" autocapitalize="none">
          <label for="password">Пароль</label>
          <input id="password" type="password" name="password" autocomplete="current-password" required maxlength="256">
          <button class="primary-action" type="submit">Войти</button>
        </form>
        <p class="sr-only" role="status" data-sign-in-status></p>
        {registration_link}"#,
            email = html(email),
            email_pattern = html(EMAIL_PATTERN)
        ),
        ClientPage::SignIn {
            email,
            error,
            registration_enabled,
            return_to,
        },
    )
}

pub(crate) fn registration_start(email: &str, error: &str) -> String {
    let (described_by, message) = form_error(error, "registration-error");
    account_page(
        "Регистрация",
        &format!(
            r#"
    <header class="auth-heading">
      <h1 id="page-title">Регистрация</h1>
      <p>Укажите почту, чтобы получить ссылку для подтверждения.</p>
    </header>
    <form method="post" action="/register"{described_by}>
      {message}
      <label for="email">Почта</label>
      <input id="email" type="email" name="email" value="{email}" autocomplete="email" required maxlength="254" pattern="{email_pattern}" spellcheck="false" autocapitalize="none">
      <button class="primary-action" type="submit">Отправить ссылку</button>
    </form>
    <p><a class="auth-link" href="/">Вернуться ко входу</a></p>"#,
            email = html(email),
            email_pattern = html(EMAIL_PATTERN)
        ),
        ClientPage::RegistrationStart { email, error },
    )
}

pub(crate) fn registration_requested() -> String {
    account_page(
        "Проверьте почту",
        r#"
    <header class="auth-heading">
      <h1 id="page-title">Проверьте почту</h1>
      <p>Если адрес доступен для регистрации, на него придёт ссылка. Откройте её на компьютере, где запущен «Север».</p>
    </header>
    <p><a class="auth-link" href="/register">Запросить ссылку ещё раз</a></p>
    <p><a class="auth-link" href="/">Вернуться ко входу</a></p>"#,
        ClientPage::RegistrationRequested,
    )
}

pub(crate) fn registration_complete(token: &str, name: &str, error: &str) -> String {
    let (described_by, message) = form_error(error, "registration-error");
    let name = String::from_utf16_lossy(&name.encode_utf16().take(240).collect::<Vec<_>>());
    account_page(
        "Завершите регистрацию",
        &format!(
            r#"
    <header class="auth-heading">
      <h1 id="page-title">Завершите регистрацию</h1>
      <p>Укажите имя и придумайте пароль для своей личной среды.</p>
    </header>
    <form method="post" action="/register/confirm"{described_by}>
      {message}
      <input type="hidden" name="token" value="{token}">
      <label for="name">Имя</label>
      <input id="name" name="name" value="{name}" autocomplete="name" required maxlength="240">
      <label for="password">Пароль</label>
      <p class="auth-hint" id="registration-password-help">От 15 до 128 символов.</p>
      <input id="password" type="password" name="password" autocomplete="new-password" aria-describedby="registration-password-help" required minlength="15" maxlength="256">
      <button class="primary-action" type="submit">Создать аккаунт</button>
    </form>
    <p><a class="auth-link" href="/">Вернуться ко входу</a></p>"#,
            token = html(token),
            name = html(&name)
        ),
        ClientPage::RegistrationComplete {
            token,
            name: &name,
            error,
        },
    )
}

pub(crate) fn no_tenant(account_name: &str, tenants: &[Tenant]) -> String {
    let mut alternatives = String::new();
    if !tenants.is_empty() {
        alternatives.push_str("<ul>");
        for tenant in tenants {
            write!(
                alternatives,
                r#"<li><a class="auth-link" href="{}">{}</a></li>"#,
                html(&spaces_path(tenant.id)),
                html(&tenant.name),
            )
            .unwrap();
        }
        alternatives.push_str("</ul>");
    }
    account_page(
        "Рабочая область",
        &format!(
            r#"
    <header class="auth-heading">
      <h1 id="page-title">Нет доступной среды</h1>
      <p>Вы вошли как <strong>{}</strong>.</p>
    </header>
    {alternatives}"#,
            html(account_name)
        ),
        ClientPage::NoTenant {
            account_name,
            tenants,
        },
    )
}
