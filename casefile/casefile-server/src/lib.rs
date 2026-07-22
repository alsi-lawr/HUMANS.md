mod api;
mod assets;
mod workbench;

use anyhow::{Context, Result};
use casefile_store::Store;
use casefile_store_sqlite::SqliteIndex;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tiny_http::Server;

fn capability() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn default_index_path(root: &Path) -> Result<PathBuf> {
    let directory = std::env::temp_dir().join("casefile-sqlite-indexes");
    fs::create_dir_all(&directory)?;
    let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
    let key = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(directory.join(format!("{key}.sqlite")))
}

pub fn serve(root: &Path, port: u16, index: Option<&Path>, write: bool) -> Result<()> {
    let root = fs::canonicalize(root).context("canonicalize planning root")?;
    let index_path = match index {
        Some(path) if path.is_absolute() => path.to_owned(),
        Some(path) => std::env::current_dir()?.join(path),
        None => default_index_path(&root)?,
    };
    let index = SqliteIndex::open(&index_path, &root)?;
    let server = Server::http(("127.0.0.1", port)).map_err(|error| anyhow::anyhow!(error))?;
    let port = server
        .server_addr()
        .to_ip()
        .context("server did not bind an IP socket")?
        .port();
    let capability = capability()?;
    println!("Casefile server: http://127.0.0.1:{port}");
    println!("Casefile root: {}", root.display());
    println!("Casefile index: {}", index_path.display());
    println!("Casefile write capability: {capability}");
    std::io::stdout().flush()?;
    let workbench = workbench::Workbench::new(Store::open(root)?, index);
    let host = api::Host::new(workbench, port, write, capability);
    for request in server.incoming_requests() {
        if let Err(error) = host.handle(request) {
            eprintln!("HTTP response failed: {error}");
        }
    }
    Ok(())
}
