mod admin;
mod cover_image;
mod db;
mod http;
mod identity;
mod markdown;
mod registration;
mod sections;
mod sections_http;
mod server;
mod settings;
mod text;
mod views;
mod workspace;
mod workspace_http;

use std::{
    env,
    io::{self, IsTerminal, Read},
    net::Ipv4Addr,
    path::Path,
    process::ExitCode,
};

use anyhow::{Context, bail, ensure};

use crate::db::Store;

const USAGE: &str = "sever <serve|migrate|import-sqlite FILE|create-account EMAIL NAME|configure-tenants FILE|configure-space-grants FILE>";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let command: Vec<&str> = arguments.iter().map(String::as_str).collect();
    if matches!(command.as_slice(), ["-h" | "--help"]) {
        println!("{USAGE}");
        return Ok(());
    }
    ensure!(
        matches!(
            command.as_slice(),
            [] | ["serve" | "migrate"]
                | [
                    "import-sqlite" | "configure-tenants" | "configure-space-grants",
                    _
                ]
                | ["create-account", _, _]
        ),
        "{USAGE}"
    );
    if matches!(command.as_slice(), ["create-account", _, _]) {
        ensure!(
            !io::stdin().is_terminal(),
            "Передайте пароль в stdin из скрытого ввода. Интерактивный ввод не поддерживается."
        );
    }
    let url =
        env::var("SEVER_DATABASE_URL").context("Укажите SEVER_DATABASE_URL для базы PostgreSQL")?;
    let store = Store::connect(&url)
        .await
        .context("Не удалось подключиться к PostgreSQL")?;
    match command.as_slice() {
        [] | ["serve"] => {
            let port = env::var("SEVER_PORT")
                .unwrap_or_else(|_| "4121".to_owned())
                .parse::<u16>()
                .context("SEVER_PORT должен быть номером локального порта")?;
            let bind_addr = env::var("SEVER_BIND_ADDR")
                .unwrap_or_else(|_| Ipv4Addr::LOCALHOST.to_string())
                .parse::<Ipv4Addr>()
                .context("SEVER_BIND_ADDR должен быть адресом IPv4")?;
            let host = env::var("SEVER_HTTP_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
            ensure!(
                matches!(host.as_str(), "127.0.0.1" | "localhost"),
                "SEVER_HTTP_HOST принимает 127.0.0.1 или localhost"
            );
            server::run(store, bind_addr, port, &host).await
        }
        ["migrate"] => store.migrate().await,
        ["import-sqlite", path] => db::import::sqlite_to_postgres(&store, Path::new(path)).await,
        ["configure-tenants", path] => admin::configure_tenants(&store, Path::new(path)).await,
        ["configure-space-grants", path] => admin::configure_grants(&store, Path::new(path)).await,
        ["create-account", email, name] => {
            let mut password = String::new();
            io::stdin()
                .take(1025)
                .read_to_string(&mut password)
                .context("Не удалось прочитать пароль из stdin")?;
            ensure!(password.len() <= 1024, "Пароль слишком длинный");
            let password = password
                .strip_suffix("\r\n")
                .or_else(|| password.strip_suffix('\n'))
                .unwrap_or(&password);
            let account =
                identity::prepare_account(email, name, password).map_err(anyhow::Error::msg)?;
            ensure!(
                store.add_account(&account).await?,
                "Учётная запись с такой почтой уже существует."
            );
            println!("{}", account.id);
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}
