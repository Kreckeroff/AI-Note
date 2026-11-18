use std::{collections::HashSet, fs::File, io::BufReader, path::Path};

use anyhow::{Context, ensure};
use serde::de::DeserializeOwned;

use crate::{
    db::Store,
    identity::normalize_email,
    settings::{GrantSettings, MembershipStatus, TenantSettings},
    text::js_trim,
};

pub(crate) async fn configure_tenants(store: &Store, path: &Path) -> anyhow::Result<()> {
    let mut config: TenantSettings = read_config(path)?;
    let mut tenant_ids = HashSet::new();
    for tenant in &mut config.tenants {
        tenant.name = js_trim(&tenant.name).to_owned();
        ensure!(
            tenant_ids.insert(tenant.id)
                && (1..=120).contains(&tenant.name.chars().count())
                && !tenant.name.chars().any(char::is_control),
            "Каждому тенанту нужны уникальный UUID и название длиной от 1 до 120 символов."
        );
    }
    let mut member_keys = HashSet::new();
    let mut currents = HashSet::new();
    for member in &mut config.memberships {
        member.email = normalize_email(&member.email).context("Проверьте адрес участника.")?;
        ensure!(
            !member.current || matches!(member.status, MembershipStatus::Active),
            "Текущий тенант требует активного членства."
        );
        ensure!(
            member_keys.insert((member.tenant_id, member.email.clone()))
                && (!member.current || currents.insert(member.email.clone())),
            "Членство указывают один раз. У аккаунта может быть один текущий тенант."
        );
    }

    store.configure_tenants(config).await?;
    println!("Настройки тенантов и членств сохранены.");
    Ok(())
}

pub(crate) async fn configure_grants(store: &Store, path: &Path) -> anyhow::Result<()> {
    let mut config: GrantSettings = read_config(path)?;
    let mut unique = HashSet::new();
    for grant in &mut config.grants {
        grant.email = normalize_email(&grant.email).context("Проверьте адрес участника.")?;
        ensure!(
            unique.insert((grant.tenant_id, grant.space_id, grant.email.clone())),
            "Назначение для одного участника и пространства указывают один раз."
        );
    }
    let count = config.grants.len();
    store.configure_grants(config).await?;
    println!("Настройки доступа сохранены. Обработано назначений: {count}.");
    Ok(())
}

fn read_config<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let file = File::open(path).context("Не удалось открыть файл настроек")?;
    // JSON syntax errors can be reported without logging submitted values.
    serde_json::from_reader(BufReader::new(file)).map_err(|error| {
        anyhow::anyhow!(
            "Проверьте формат файла настроек (строка {}, столбец {}).",
            error.line(),
            error.column()
        )
    })
}
