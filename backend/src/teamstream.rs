use std::env::{self, VarError};

use anyhow::{Context, ensure};
use url::{Host, Position, Url};
use uuid::Uuid;

pub(crate) const PROVIDER: &str = "https://workspace.teamstream.mws.ru";
const PROVIDER_HOST: &str = "workspace.teamstream.mws.ru";

pub(crate) struct Video {
    id: Uuid,
    start: Option<u32>,
}

impl Video {
    pub(crate) fn from_address(address: &Url) -> Option<Self> {
        if address.scheme() != "https"
            || address.host_str() != Some(PROVIDER_HOST)
            || address.port().is_some()
            || !address.username().is_empty()
            || address.password().is_some()
            || address.fragment().is_some()
        {
            return None;
        }
        Self::from_target(address.path(), address.query())
    }

    pub(crate) fn from_target(path: &str, query: Option<&str>) -> Option<Self> {
        let id = path.strip_prefix("/embed/")?;
        let parsed = Uuid::try_parse(id).ok()?;
        if !id.eq_ignore_ascii_case(&parsed.hyphenated().to_string()) {
            return None;
        }
        let start = match query {
            None => None,
            Some(query) => {
                let seconds = query.strip_prefix("t=")?;
                if seconds.is_empty() || !seconds.bytes().all(|byte| byte.is_ascii_digit()) {
                    return None;
                }
                Some(seconds.parse().ok()?)
            }
        };
        Some(Self { id: parsed, start })
    }

    pub(crate) fn provider_url(&self) -> String {
        let mut url = format!("{PROVIDER}/embed/{}", self.id);
        if let Some(seconds) = self.start {
            url.push_str(&format!("?t={seconds}"));
        }
        url
    }
}

pub(crate) fn bare_origin(address: &str) -> Option<Url> {
    if address
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control() || byte == b'\\')
    {
        return None;
    }
    let (_, authority) = address.split_once("://")?;
    if authority.starts_with('/') || authority.contains('@') {
        return None;
    }
    let url = Url::parse(address).ok()?;
    let local = match url.host()? {
        Host::Domain(host)
            if !host.split('.').all(|label| {
                !label.is_empty()
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            }) =>
        {
            return None;
        }
        Host::Domain(host) => host == "localhost",
        Host::Ipv4(host) => host.is_loopback(),
        Host::Ipv6(host) => host.is_loopback(),
    };
    if !(url.scheme() == "https" || url.scheme() == "http" && local)
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    // Reject paths that the URL parser may have normalized down to "/".
    if !matches!(authority.split_once('/'), None | Some((_, ""))) {
        return None;
    }
    Some(url)
}

#[derive(Clone)]
pub(crate) struct IsolatedOrigin(Url);

impl IsolatedOrigin {
    pub(crate) fn parse(address: &str) -> Option<Self> {
        let url = bare_origin(address)?;
        (url.host_str() != Some(PROVIDER_HOST)).then_some(Self(url))
    }

    fn is_separate_from(&self, app: &str) -> bool {
        Url::parse(app).is_ok_and(|app| {
            app.host_str()
                .is_some_and(|host| self.0.host_str() != Some(host.trim_end_matches('.')))
        })
    }

    pub(crate) fn from_env(app: &str) -> anyhow::Result<Option<Self>> {
        let value = match env::var("SEVER_TEAMSTREAM_ORIGIN") {
            Ok(value) if value.is_empty() => return Ok(None),
            Ok(value) => value,
            Err(VarError::NotPresent) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let origin = Self::parse(&value).context(
            "SEVER_TEAMSTREAM_ORIGIN должен задавать HTTPS origin или локальный HTTP origin",
        )?;
        ensure!(
            origin.is_separate_from(app),
            "Для плеера TeamStream требуется отдельный хост"
        );
        Ok(Some(origin))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0[..Position::BeforePath]
    }

    pub(crate) fn authority(&self) -> &str {
        &self.0[Position::BeforeHost..Position::AfterPort]
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    const APP: &str = "http://localhost:4125";
    const PLAYER: &str = "http://127.0.0.1:4124";
    const ID: &str = "abcd1234-0000-4000-8000-000000000001";

    #[test]
    fn validates_normalized_provider_path_and_seconds() {
        let upper = ID.to_uppercase();
        let valid = Url::parse(&format!("{PROVIDER}/embed/{upper}?t=030")).unwrap();
        let video = Video::from_address(&valid).unwrap();
        assert_eq!(video.provider_url(), format!("{PROVIDER}/embed/{ID}?t=30"));
        for link in [
            format!("{PROVIDER}/embed/{ID}?"),
            format!("{PROVIDER}/embed/{ID}#"),
            format!("{PROVIDER}/embed/{ID}?t=+1"),
            format!("{PROVIDER}/embed/{ID}?t=1&t=1"),
            format!("{PROVIDER}/embed/{ID}?t=%31"),
            format!("{PROVIDER}/embed/{ID}?t=4294967296"),
            format!("{PROVIDER}/embed/{ID}?url=https://example.test"),
            format!("{PROVIDER}:444/embed/{ID}"),
            format!("{PROVIDER}./embed/{ID}"),
            format!("{PROVIDER}.example.test/embed/{ID}"),
            format!("{PROVIDER}/video/{ID}"),
            format!("{PROVIDER}/embed/{ID}/"),
            format!("{PROVIDER}/embed/{}", ID.replace('-', "")),
        ] {
            assert!(
                Video::from_address(&Url::parse(&link).unwrap()).is_none(),
                "{link}"
            );
        }
    }

    #[test]
    fn accepts_bare_secure_origins_on_distinct_hosts() {
        let wrapper = IsolatedOrigin::parse(PLAYER).unwrap();
        assert!(wrapper.is_separate_from(APP));
        assert!(!wrapper.is_separate_from("http://127.0.0.1:4125"));
        assert!(!wrapper.is_separate_from("http://127.0.0.1.:4125"));
        assert!(IsolatedOrigin::parse("https://video.example.test/").is_some());
        assert!(IsolatedOrigin::parse(PROVIDER).is_none());
        for bad in [
            "http://video.example.test",
            "https://video.example.test/path",
            "https://video.example.test?",
            "https://video.example.test#",
            "https://user@video.example.test",
            "https://@video.example.test",
            "https:///video.example.test",
            "https://video.example.test.",
            "https://video.example.test\\",
            "https://video.example.test;other",
            "https://video..example.test",
        ] {
            assert!(IsolatedOrigin::parse(bad).is_none(), "{bad}");
        }
    }
}
