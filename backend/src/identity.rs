use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use axum::http::{HeaderMap, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use regex::Regex;
use scrypt::{Params, scrypt};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

pub(crate) use crate::clock::now_ms;

use crate::{
    db::{NewAccount, NewSession, Principal, Store},
    text::js_trim,
};

const SESSION_LIFETIME_MS: i64 = 12 * 60 * 60 * 1000;
const ATTEMPT_WINDOW_MS: i64 = 5 * 60 * 1000;
const ACCOUNT_ATTEMPT_LIMIT: u8 = 6;
const SERVICE_ATTEMPT_LIMIT: u8 = 48;
const CONCURRENT_ATTEMPT_LIMIT: usize = 4;
pub(crate) const EMAIL_MAX_LENGTH: usize = 254;

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"\A[a-zA-Z0-9.!#$%&'*+\x2f=?^_\x60\x7b\x7c\x7d~\x2d]{1,64}",
        r"@[a-zA-Z0-9](?:[a-zA-Z0-9\x2d]{0,61}[a-zA-Z0-9])?",
        r"(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9\x2d]{0,61}[a-zA-Z0-9])?)+\z",
    ))
    .expect("valid registration email pattern")
});

pub(crate) fn normalize_email(email: &str) -> Option<String> {
    let email = js_trim(email);
    (email.len() <= EMAIL_MAX_LENGTH && EMAIL.is_match(email)).then(|| email.to_ascii_lowercase())
}

fn valid_password(password: &str) -> bool {
    !password.is_empty()
        && !password.contains(['\r', '\n', '\0'])
        && password.chars().count() <= 128
}

fn derive_password(password: &str, salt: &[u8]) -> [u8; 32] {
    let params = Params::new(17, 8, 1, 32).expect("valid legacy scrypt parameters");
    let mut hash = [0; 32];
    scrypt(password.as_bytes(), salt, &params, &mut hash).expect("valid scrypt output length");
    hash
}

pub(crate) fn prepare_account(
    email: &str,
    name: &str,
    password: &str,
) -> Result<NewAccount, &'static str> {
    let email = normalize_email(email).ok_or("Укажите действительный адрес электронной почты.")?;
    let name = js_trim(name);
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err("Укажите имя длиной от 1 до 120 символов.");
    }
    if !valid_password(password) || password.chars().count() < 15 {
        return Err("Пароль должен содержать от 15 до 128 символов в одной строке.");
    }
    let mut salt = [0; 16];
    OsRng.fill_bytes(&mut salt);
    Ok(NewAccount {
        id: Uuid::new_v4(),
        email,
        name: name.to_owned(),
        password_hash: derive_password(password, &salt).to_vec(),
        salt: salt.to_vec(),
    })
}

pub(crate) fn random_token() -> String {
    let mut token = [0; 32];
    OsRng.fill_bytes(&mut token);
    URL_SAFE_NO_PAD.encode(token)
}

pub(crate) fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

pub(crate) fn valid_token(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn prepare_session(now: i64) -> (String, NewSession) {
    let token = random_token();
    let digest = token_digest(&token).to_vec();
    (
        token,
        NewSession {
            digest,
            created_at: now,
            expires_at: now + SESSION_LIFETIME_MS,
        },
    )
}

pub(crate) fn session_cookie(token: &str) -> String {
    let max_age = SESSION_LIFETIME_MS / 1000;
    format!("sever_session={token}; Path=/workspace; HttpOnly; SameSite=Strict; Max-Age={max_age}")
}

pub(crate) async fn identify(
    store: &Store,
    headers: &HeaderMap,
) -> Result<Option<Principal>, sqlx::Error> {
    let Some(digest) = session_digest(headers) else {
        return Ok(None);
    };
    store.principal(&digest, now_ms()).await
}

pub(crate) fn session_digest(headers: &HeaderMap) -> Option<[u8; 32]> {
    let mut tokens = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|entry| js_trim(entry).strip_prefix("sever_session="));
    let token = tokens.next().filter(|token| valid_token(token))?;
    tokens.next().is_none().then(|| token_digest(token))
}

#[derive(Default)]
struct AttemptWindow {
    end: i64,
    count: u8,
}

impl AttemptWindow {
    fn restart(&mut self, now: i64) {
        *self = Self {
            end: now + ATTEMPT_WINDOW_MS,
            count: 0,
        };
    }

    fn retry_after(&self, now: i64) -> u64 {
        ((self.end - now + 999) / 1000) as u64
    }
}

#[derive(Default)]
struct AttemptBudget {
    by_email: HashMap<String, AttemptWindow>,
    service: AttemptWindow,
}

pub(crate) enum LoginOutcome {
    Accepted { token: String, account_id: Uuid },
    Invalid,
    Limited(u64),
}

pub(crate) struct Identity {
    budget: Mutex<AttemptBudget>,
    concurrency: Arc<Semaphore>,
    dummy_salt: [u8; 16],
    dummy_hash: [u8; 32],
}

impl Identity {
    pub(crate) fn new() -> Self {
        let mut dummy_salt = [0; 16];
        let mut dummy_hash = [0; 32];
        OsRng.fill_bytes(&mut dummy_salt);
        OsRng.fill_bytes(&mut dummy_hash);
        Self {
            budget: Mutex::new(AttemptBudget::default()),
            concurrency: Arc::new(Semaphore::new(CONCURRENT_ATTEMPT_LIMIT)),
            dummy_salt,
            dummy_hash,
        }
    }

    fn reserve(&self, email: &str, now: i64) -> Result<OwnedSemaphorePermit, u64> {
        let mut budget = self.budget.lock().expect("login budget lock");
        if now >= budget.service.end {
            budget.service.restart(now);
            budget.by_email.retain(|_, window| now < window.end);
        }
        if let Some(window) = budget.by_email.get(email)
            && now < window.end
            && window.count >= ACCOUNT_ATTEMPT_LIMIT
        {
            return Err(window.retry_after(now));
        }
        if budget.service.count >= SERVICE_ATTEMPT_LIMIT {
            return Err(budget.service.retry_after(now));
        }
        let permit = self
            .concurrency
            .clone()
            .try_acquire_owned()
            .map_err(|_| 2_u64)?;
        let window = budget.by_email.entry(email.to_owned()).or_default();
        if now >= window.end {
            window.restart(now);
        }
        window.count += 1;
        budget.service.count += 1;
        Ok(permit)
    }

    pub(crate) async fn sign_in(
        &self,
        store: &Store,
        email: &str,
        password: &str,
    ) -> Result<LoginOutcome, sqlx::Error> {
        let Some(email) = normalize_email(email).filter(|_| valid_password(password)) else {
            return Ok(LoginOutcome::Invalid);
        };
        let permit = match self.reserve(&email, now_ms()) {
            Ok(permit) => permit,
            Err(retry_after) => return Ok(LoginOutcome::Limited(retry_after)),
        };

        let (account_id, salt, expected) = match store.credentials(&email).await? {
            Some(account) => (Some(account.id), account.salt, account.password_hash),
            None => (None, self.dummy_salt.to_vec(), self.dummy_hash.to_vec()),
        };
        let password = password.to_owned();
        let store = store.clone();
        // The permit remains held through hashing and session creation if the client disconnects.
        tokio::spawn(async move {
            let _permit = permit;
            let matches = tokio::task::spawn_blocking(move || {
                bool::from(derive_password(&password, &salt).ct_eq(expected.as_slice()))
            })
            .await
            .map_err(|_| sqlx::Error::Protocol("password verification task failed".into()))?;
            let Some(account_id) = account_id.filter(|_| matches) else {
                return Ok(LoginOutcome::Invalid);
            };
            let (token, session) = prepare_session(now_ms());
            store.add_session(account_id, &session).await?;
            Ok(LoginOutcome::Accepted { token, account_id })
        })
        .await
        .map_err(|_| sqlx::Error::Protocol("sign-in task failed".into()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrated_scrypt_hash_and_session_digest_match_node() {
        let salt = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        assert_eq!(
            hex(&derive_password("Русский пароль 🔑 для теста", &salt)),
            "3c7483a1471ea301ff504f6cd4e8e29c1d9aafee4243b6f4126fa2db5c3367ed",
        );
        assert_eq!(
            hex(&token_digest("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")),
            "0f007385b6f9d4b7eeb2748605afe1a984a0a3bfa3f014d09e2a784ce9e5cd1a",
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
