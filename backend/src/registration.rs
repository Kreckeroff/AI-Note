use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Result as AnyResult, anyhow, bail};
use axum::{
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::Response,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use tokio::{io::AsyncWriteExt, process::Command, time::timeout};
use uuid::Uuid;

use crate::{
    db::Store,
    http::{Form, FormError, check_origin, html, plain, read_form, redirect},
    identity::{
        EMAIL_MAX_LENGTH, normalize_email, now_ms, prepare_account, prepare_session, random_token,
        session_cookie, token_digest, valid_token,
    },
    views,
};

const CONFIRMATION_PATH: &str = "/register/confirm";
const CONFIRMATION_LIFETIME_MINUTES: i64 = 30;
const SEND_WINDOW_MS: i64 = 60 * 1000;
const SEND_LIMIT: u8 = 24;
const PENDING_LIMIT: usize = 200;
const RESTART_REGISTRATION: &str = "Ссылка недействительна. Начните регистрацию заново.";
const OPEN_REGISTRATION: &str = "Откройте форму регистрации в «Север».";

#[derive(Clone)]
struct MailDelivery {
    executable: PathBuf,
    from: String,
}

impl MailDelivery {
    fn from_env() -> AnyResult<Self> {
        let executable =
            PathBuf::from(env::var_os("SEVER_REGISTRATION_SENDMAIL").unwrap_or_default());
        if !is_executable(&executable) {
            bail!("В SEVER_REGISTRATION_SENDMAIL укажите абсолютный путь к исполняемому sendmail.");
        }
        let from = env::var("SEVER_REGISTRATION_FROM").unwrap_or_default();
        let from = normalize_email(&from).ok_or_else(|| {
            anyhow!("В SEVER_REGISTRATION_FROM укажите действительный адрес отправителя.")
        })?;
        Ok(Self { executable, from })
    }

    async fn send(&self, email: &str, link: &str) -> io::Result<()> {
        let subject = STANDARD.encode("Подтвердите почту для регистрации в «Севере»");
        let message = format!(
            "From: <{}>\nTo: <{email}>\nSubject: =?UTF-8?B?{subject}?=\n\
             MIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\n\
             Content-Transfer-Encoding: 8bit\n\n\
             Чтобы завершить регистрацию в «Севере», откройте ссылку на компьютере, где запущено приложение.\n\
             {link}\nСрок действия ссылки — {CONFIRMATION_LIFETIME_MINUTES} мин. Если вы не запрашивали регистрацию, письмо можно проигнорировать.\n",
            self.from
        );
        let mut child = Command::new(&self.executable)
            .args(["-i", "-t"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        timeout(Duration::from_secs(5), async {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| io::Error::other("sendmail stdin unavailable"))?;
            stdin.write_all(message.as_bytes()).await?;
            drop(stdin);
            if child.wait().await?.success() {
                Ok(())
            } else {
                Err(io::Error::other("sendmail rejected message"))
            }
        })
        .await?
    }
}

fn is_executable(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    metadata.is_file()
}

type TokenDigest = [u8; 32];

struct Pending {
    email: String,
    expires_at: i64,
}

#[derive(Default)]
struct Requests {
    pending: HashMap<TokenDigest, Pending>,
    cooldowns: HashMap<String, i64>,
    window_end: i64,
    sends: u8,
    completing: Option<TokenDigest>,
}

impl Requests {
    fn reserve_send(&mut self, email: &str, now: i64) -> bool {
        self.pending.retain(|_, pending| pending.expires_at > now);
        self.cooldowns.retain(|_, allowed_at| *allowed_at > now);
        if now >= self.window_end {
            self.window_end = now + SEND_WINDOW_MS;
            self.sends = 0;
        }
        if self.pending.len() >= PENDING_LIMIT
            || self.sends >= SEND_LIMIT
            || self.cooldowns.contains_key(email)
        {
            return false;
        }
        self.sends += 1;
        self.cooldowns
            .insert(email.to_owned(), now + SEND_WINDOW_MS);
        true
    }

    fn find_pending(&self, digest: &TokenDigest, now: i64) -> Option<&Pending> {
        self.pending
            .get(digest)
            .filter(|pending| pending.expires_at > now)
    }
}

struct Completion {
    requests: Arc<Mutex<Requests>>,
    digest: TokenDigest,
}

impl Completion {
    fn is_pending(&self) -> bool {
        self.requests
            .lock()
            .expect("registration state lock")
            .find_pending(&self.digest, now_ms())
            .is_some()
    }

    fn invalidate_recipient(&self, email: &str) {
        self.requests
            .lock()
            .expect("registration state lock")
            .pending
            .retain(|_, pending| pending.email != email);
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        self.requests
            .lock()
            .expect("registration state lock")
            .completing = None;
    }
}

enum ConfirmationOutcome {
    Accepted {
        token: String,
        account_id: Uuid,
        tenant_id: Uuid,
    },
    InvalidLink,
    InvalidFields(&'static str),
    Limited,
}

pub(crate) struct Registration {
    requests: Arc<Mutex<Requests>>,
    mail: MailDelivery,
}

impl Registration {
    pub(crate) fn from_env() -> AnyResult<Option<Self>> {
        match env::var_os("SEVER_SELF_REGISTRATION").as_deref() {
            None => Ok(None),
            Some(value) if value == OsStr::new("false") => Ok(None),
            Some(value) if value == OsStr::new("true") => Ok(Some(Self {
                requests: Arc::new(Mutex::new(Requests::default())),
                mail: MailDelivery::from_env()?,
            })),
            Some(_) => bail!("SEVER_SELF_REGISTRATION принимает только true или false."),
        }
    }

    async fn request_link(
        &self,
        store: &Store,
        email: String,
        origin: &str,
    ) -> Result<(), sqlx::Error> {
        let now = now_ms();
        let allowed = self
            .requests
            .lock()
            .expect("registration state lock")
            .reserve_send(&email, now);
        if !allowed {
            eprintln!(r#"{{"event":"registration.requested","outcome":"limited"}}"#);
            return Ok(());
        }
        eprintln!(r#"{{"event":"registration.requested","outcome":"accepted"}}"#);
        if store.credentials(&email).await?.is_some() {
            return Ok(());
        }

        let token = random_token();
        let digest = token_digest(&token);
        {
            let mut requests = self.requests.lock().expect("registration state lock");
            if requests.pending.len() >= PENDING_LIMIT {
                return Ok(());
            }
            requests.pending.insert(
                digest,
                Pending {
                    email: email.clone(),
                    expires_at: now + CONFIRMATION_LIFETIME_MINUTES * 60 * 1000,
                },
            );
        }
        let link = format!("{origin}{CONFIRMATION_PATH}?token={token}");
        let requests = Arc::clone(&self.requests);
        let mail = self.mail.clone();
        tokio::spawn(async move {
            if mail.send(&email, &link).await.is_ok() {
                eprintln!(r#"{{"event":"registration.delivery","outcome":"accepted"}}"#);
            } else {
                requests
                    .lock()
                    .expect("registration state lock")
                    .pending
                    .remove(&digest);
                eprintln!(r#"{{"event":"registration.delivery","outcome":"failed"}}"#);
            }
        });
        Ok(())
    }

    fn is_pending(&self, token: &str) -> bool {
        valid_token(token)
            && self
                .requests
                .lock()
                .expect("registration state lock")
                .find_pending(&token_digest(token), now_ms())
                .is_some()
    }

    fn begin_completion(&self, token: &str) -> Result<(Completion, String), ConfirmationOutcome> {
        if !valid_token(token) {
            return Err(ConfirmationOutcome::InvalidLink);
        }
        let digest = token_digest(token);
        let mut requests = self.requests.lock().expect("registration state lock");
        let Some(pending) = requests.find_pending(&digest, now_ms()) else {
            return Err(ConfirmationOutcome::InvalidLink);
        };
        if let Some(active_digest) = requests.completing {
            return Err(if active_digest == digest {
                ConfirmationOutcome::InvalidLink
            } else {
                ConfirmationOutcome::Limited
            });
        }
        let email = pending.email.clone();
        requests.completing = Some(digest);
        let completion = Completion {
            requests: Arc::clone(&self.requests),
            digest,
        };
        Ok((completion, email))
    }

    async fn confirm(
        &self,
        store: &Store,
        token: &str,
        name: &str,
        password: &str,
    ) -> Result<ConfirmationOutcome, sqlx::Error> {
        let (completion, email) = match self.begin_completion(token) {
            Ok(values) => values,
            Err(outcome) => return Ok(outcome),
        };
        let name = name.to_owned();
        let password = password.to_owned();
        let store = store.clone();
        // Complete the claimed link even if the client disconnects while scrypt is running.
        tokio::spawn(async move {
            let account =
                tokio::task::spawn_blocking(move || prepare_account(&email, &name, &password))
                    .await
                    .map_err(|_| sqlx::Error::Protocol("account preparation task failed".into()))?;
            let account = match account {
                Ok(account) => account,
                Err(message) => return Ok(ConfirmationOutcome::InvalidFields(message)),
            };
            if !completion.is_pending() {
                return Ok(ConfirmationOutcome::InvalidLink);
            }
            let (token, session) = prepare_session(now_ms());
            let tenant_id = store.register(&account, &session).await?;
            completion.invalidate_recipient(&account.email);
            match tenant_id {
                Some(tenant_id) => Ok(ConfirmationOutcome::Accepted {
                    token,
                    account_id: account.id,
                    tenant_id,
                }),
                None => Ok(ConfirmationOutcome::InvalidLink),
            }
        })
        .await
        .map_err(|_| sqlx::Error::Protocol("registration task failed".into()))?
    }

    pub(crate) async fn handle(
        &self,
        store: &Store,
        origin: &str,
        request: Request,
    ) -> Result<Response, sqlx::Error> {
        let confirming = request.uri().path() == CONFIRMATION_PATH;
        if matches!(*request.method(), Method::GET | Method::HEAD) {
            if !confirming {
                return Ok(html(StatusCode::OK, views::registration_start("", "")));
            }
            let query = Form::from_query(request.uri().query().unwrap_or_default());
            let token = query.get("token");
            return Ok(if self.is_pending(token) {
                confirmation_page(StatusCode::OK, token, "", "")
            } else {
                invalid_link()
            });
        }
        if request.method() != Method::POST {
            let mut response = private(plain(StatusCode::METHOD_NOT_ALLOWED, "Метод недоступен."));
            response
                .headers_mut()
                .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD, POST"));
            return Ok(response);
        }
        if !check_origin(request.headers(), origin) {
            return Ok(private(plain(StatusCode::FORBIDDEN, OPEN_REGISTRATION)));
        }
        let form = match read_form(request).await {
            Ok(form) => form,
            Err(error) => return Ok(form_error(error)),
        };
        if !confirming {
            let entered_email = form.get("email");
            let Some(email) = normalize_email(entered_email) else {
                let shown_email: String = entered_email.chars().take(EMAIL_MAX_LENGTH).collect();
                return Ok(html(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    views::registration_start(&shown_email, "Укажите действительный адрес почты."),
                ));
            };
            self.request_link(store, email, origin).await?;
            return Ok(html(StatusCode::ACCEPTED, views::registration_requested()));
        }
        let token = form.get("token");
        let name = form.get("name");
        let response = match self
            .confirm(store, token, name, form.get("password"))
            .await?
        {
            ConfirmationOutcome::InvalidLink => invalid_link(),
            ConfirmationOutcome::InvalidFields(message) => {
                confirmation_page(StatusCode::UNPROCESSABLE_ENTITY, token, name, message)
            }
            ConfirmationOutcome::Limited => {
                let mut response = confirmation_page(
                    StatusCode::TOO_MANY_REQUESTS,
                    token,
                    name,
                    "Подождите несколько секунд и попробуйте снова.",
                );
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
                response
            }
            ConfirmationOutcome::Accepted {
                token,
                account_id,
                tenant_id,
            } => {
                eprintln!(
                    r#"{{"event":"registration.completed","accountId":"{account_id}","tenantId":"{tenant_id}"}}"#
                );
                let mut response = private(redirect("/workspace"));
                response.headers_mut().insert(
                    header::SET_COOKIE,
                    HeaderValue::from_str(&session_cookie(&token)).expect("valid session cookie"),
                );
                response
            }
        };
        Ok(response)
    }
}

fn private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

fn invalid_link() -> Response {
    private(plain(StatusCode::NOT_FOUND, RESTART_REGISTRATION))
}

fn confirmation_page(status: StatusCode, token: &str, name: &str, error: &str) -> Response {
    // Preserve Origin on form submissions while omitting the confirmation query from Referer.
    let mut response = html(status, views::registration_complete(token, name, error));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin"),
    );
    response
}

fn form_error(error: FormError) -> Response {
    let mut response = match error {
        FormError::WrongEncoding => plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, OPEN_REGISTRATION),
        FormError::TooLarge => plain(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Форма регистрации слишком большая.",
        ),
        FormError::Unreadable => plain(StatusCode::BAD_REQUEST, OPEN_REGISTRATION),
    };
    if matches!(error, FormError::TooLarge) {
        response
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("close"));
    }
    private(response)
}
