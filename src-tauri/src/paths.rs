use crate::error::Result;
use crate::model::{Account, Accounts, Instance, Settings};
use std::path::PathBuf;

/// Все данные лаунчера: настройки, инстансы, общий кэш библиотек и ассетов.
/// BUBANYA_DATA_DIR позволяет запустить отдельную копию (портативный режим, тесты).
pub fn root() -> PathBuf {
    if let Some(path) = std::env::var_os("BUBANYA_DATA_DIR") {
        return PathBuf::from(path);
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Bubanya")
}

pub fn meta() -> PathBuf {
    root().join("meta")
}

pub fn instances() -> PathBuf {
    root().join("instances")
}

pub fn settings_file() -> PathBuf {
    root().join("settings.json")
}

pub fn instances_file() -> PathBuf {
    root().join("instances.json")
}

pub fn accounts_file() -> PathBuf {
    root().join("accounts.json")
}

pub fn ensure() -> Result<()> {
    migrate_legacy();
    std::fs::create_dir_all(meta())?;
    std::fs::create_dir_all(instances())?;
    Ok(())
}

/// Один раз переносит данные старого OneLauncher в Bubanya.
fn migrate_legacy() {
    let Some(data) = dirs::data_dir() else {
        return;
    };
    let old = data.join("OneLauncher");
    let new = data.join("Bubanya");
    if new.exists() || !old.exists() {
        return;
    }
    if std::fs::rename(&old, &new).is_ok() {
        return;
    }
    let _ = copy_dir(&old, &new);
}

fn copy_dir(from: &PathBuf, to: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let destination = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir(&path, &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

pub fn load_settings() -> Settings {
    std::fs::read_to_string(settings_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    ensure()?;
    std::fs::write(settings_file(), serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

pub fn load_accounts() -> Accounts {
    if let Ok(text) = std::fs::read_to_string(accounts_file())
        && let Ok(mut accounts) = serde_json::from_str::<Accounts>(&text)
    {
        let active_ok = accounts
            .active_id
            .as_ref()
            .is_some_and(|id| accounts.accounts.iter().any(|account| &account.id == id));
        if !active_ok {
            accounts.active_id = accounts.accounts.first().map(|account| account.id.clone());
        }
        return accounts;
    }
    let settings = load_settings();
    let name = if settings.nickname.trim().is_empty() {
        "Player".to_string()
    } else {
        settings.nickname.trim().to_string()
    };
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        kind: "offline".to_string(),
        uuid: crate::auth::offline_uuid(&name),
        xuid: None,
        name,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0),
    };
    let accounts = Accounts {
        active_id: Some(account.id.clone()),
        accounts: vec![account],
    };
    let _ = save_accounts(&accounts);
    accounts
}

pub fn save_accounts(accounts: &Accounts) -> Result<()> {
    ensure()?;
    std::fs::write(accounts_file(), serde_json::to_string_pretty(accounts)?)?;
    Ok(())
}

pub fn load_instances() -> Vec<Instance> {
    std::fs::read_to_string(instances_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_instances(list: &[Instance]) -> Result<()> {
    ensure()?;
    std::fs::write(instances_file(), serde_json::to_string_pretty(list)?)?;
    Ok(())
}

pub fn ensure_instance(instance: &Instance) -> Result<()> {
    std::fs::create_dir_all(instances().join(&instance.id).join("game"))?;
    Ok(())
}

pub fn instance_dir(id: &str) -> PathBuf {
    instances().join(id)
}

/// Живые тесты, меняющие BUBANYA_DATA_DIR, должны держать этот лок,
/// иначе параллельные прогоны перетирают каталоги данных друг друга.
#[cfg(test)]
pub(crate) static DATA_DIR_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
