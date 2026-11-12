mod auth;
mod content;
mod download;
mod error;
mod fabric;
mod forge;
mod java;
mod launch;
mod microsoft;
mod model;
mod modrinth;
mod mojang;
mod paths;
mod process;
mod token_store;

use model::{
    Account, Accounts, Instance, JavaInfo, LaunchAuth, LaunchInfo, Settings, VersionSummary,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::State;

pub struct AppState {
    pub client: reqwest::Client,
    /// Отдельный клиент для Microsoft/Xbox: HTTP/1.1 (на некоторых сетях HTTP/2 к Xbox Live не проходит).
    pub auth_client: reqwest::Client,
    pub settings: Mutex<Settings>,
    pub instances: Mutex<Vec<Instance>>,
    pub accounts: Mutex<Accounts>,
    pub microsoft_tokens: Mutex<HashMap<String, microsoft::MinecraftAuth>>,
    pub downloads: Mutex<HashMap<String, Arc<download::DownloadControl>>>,
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> Result<(), error::Error> {
    paths::save_settings(&settings)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn list_instances(state: State<AppState>) -> Vec<Instance> {
    state.instances.lock().unwrap().clone()
}

#[tauri::command]
fn list_accounts(state: State<AppState>) -> Accounts {
    state.accounts.lock().unwrap().clone()
}

fn validate_nickname(value: &str) -> Result<String, error::Error> {
    let name = value.trim();
    let length = name.chars().count();
    if !(3..=16).contains(&length) {
        return Err(error::Error::Msg(
            "Ник должен быть от 3 до 16 символов".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(error::Error::Msg(
            "В нике можно использовать только латиницу, цифры и _".to_string(),
        ));
    }
    Ok(name.to_string())
}

fn sync_active_nickname(state: &State<AppState>, accounts: &Accounts) {
    let name = accounts.active().map(|account| account.name.clone());
    if let Some(name) = name {
        let mut settings = state.settings.lock().unwrap();
        if settings.nickname != name {
            settings.nickname = name;
            let _ = paths::save_settings(&settings);
        }
    }
}

#[tauri::command]
fn add_offline_account(state: State<AppState>, name: String) -> Result<Accounts, error::Error> {
    let name = validate_nickname(&name)?;
    let mut accounts = state.accounts.lock().unwrap();
    if accounts
        .accounts
        .iter()
        .any(|account| account.name.eq_ignore_ascii_case(&name))
    {
        return Err(error::Error::Msg("Такой аккаунт уже есть".to_string()));
    }
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        kind: "offline".to_string(),
        uuid: auth::offline_uuid(&name),
        xuid: None,
        name,
        created_at: now_millis(),
    };
    accounts.accounts.push(account.clone());
    if accounts.active_id.is_none() {
        accounts.active_id = Some(account.id);
    }
    paths::save_accounts(&accounts)?;
    let result = accounts.clone();
    drop(accounts);
    sync_active_nickname(&state, &result);
    Ok(result)
}

#[tauri::command]
fn rename_account(
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<Accounts, error::Error> {
    let name = validate_nickname(&name)?;
    let mut accounts = state.accounts.lock().unwrap();
    if accounts
        .accounts
        .iter()
        .any(|account| account.id != id && account.name.eq_ignore_ascii_case(&name))
    {
        return Err(error::Error::Msg("Такой аккаунт уже есть".to_string()));
    }
    let Some(account) = accounts
        .accounts
        .iter_mut()
        .find(|account| account.id == id)
    else {
        return Err(error::Error::Msg("Аккаунт не найден".to_string()));
    };
    account.name = name.clone();
    account.uuid = auth::offline_uuid(&name);
    paths::save_accounts(&accounts)?;
    let result = accounts.clone();
    drop(accounts);
    sync_active_nickname(&state, &result);
    Ok(result)
}

#[tauri::command]
fn delete_account(state: State<AppState>, id: String) -> Result<Accounts, error::Error> {
    let mut accounts = state.accounts.lock().unwrap();
    accounts.accounts.retain(|account| account.id != id);
    if accounts.active_id.as_deref() == Some(id.as_str()) {
        accounts.active_id = accounts.accounts.first().map(|account| account.id.clone());
    }
    paths::save_accounts(&accounts)?;
    let result = accounts.clone();
    drop(accounts);
    state.microsoft_tokens.lock().unwrap().remove(&id);
    token_store::delete_refresh(&id);
    sync_active_nickname(&state, &result);
    Ok(result)
}

#[tauri::command]
fn set_active_account(state: State<AppState>, id: String) -> Result<Accounts, error::Error> {
    let mut accounts = state.accounts.lock().unwrap();
    if !accounts.accounts.iter().any(|account| account.id == id) {
        return Err(error::Error::Msg("Аккаунт не найден".to_string()));
    }
    accounts.active_id = Some(id);
    paths::save_accounts(&accounts)?;
    let result = accounts.clone();
    drop(accounts);
    sync_active_nickname(&state, &result);
    Ok(result)
}

#[tauri::command]
async fn get_account_skin(
    state: State<'_, AppState>,
    uuid: String,
) -> Result<Option<String>, error::Error> {
    microsoft::skin_head(&state.auth_client, &uuid).await
}

#[tauri::command]
async fn microsoft_begin_login(
    state: State<'_, AppState>,
) -> Result<microsoft::DeviceCodeInfo, error::Error> {
    microsoft::begin(&state.auth_client).await
}

#[tauri::command]
async fn microsoft_poll_login(
    state: State<'_, AppState>,
    device_code: String,
) -> Result<microsoft::PollResult, error::Error> {
    match microsoft::poll_token(&state.auth_client, &device_code).await? {
        microsoft::PollOutcome::Pending => Ok(microsoft::PollResult {
            status: "pending".to_string(),
            message: "Ждём подтверждения…".to_string(),
            accounts: None,
        }),
        microsoft::PollOutcome::Error(message) => Ok(microsoft::PollResult {
            status: "error".to_string(),
            message,
            accounts: None,
        }),
        microsoft::PollOutcome::Success(token) => {
            let auth = match microsoft::complete(&state.auth_client, token).await {
                Ok(auth) => auth,
                Err(error) => {
                    return Ok(microsoft::PollResult {
                        status: "error".to_string(),
                        message: error.to_string(),
                        accounts: None,
                    });
                }
            };
            let mut accounts = state.accounts.lock().unwrap();
            let uuid = microsoft::dashed_uuid(&auth.profile.id);
            let existing = accounts
                .accounts
                .iter()
                .find(|account| account.kind == "microsoft" && account.uuid == uuid)
                .cloned();
            let account = microsoft::make_account(&auth, existing.as_ref());
            if let Some(index) = accounts
                .accounts
                .iter()
                .position(|item| item.id == account.id)
            {
                accounts.accounts[index] = account.clone();
            } else {
                accounts.accounts.push(account.clone());
            }
            accounts.active_id = Some(account.id.clone());
            paths::save_accounts(&accounts)?;
            let result = accounts.clone();
            drop(accounts);
            state
                .microsoft_tokens
                .lock()
                .unwrap()
                .insert(account.id.clone(), auth.clone());
            token_store::store_refresh(&account.id, &auth.refresh_token);
            sync_active_nickname(&state, &result);
            Ok(microsoft::PollResult {
                status: "success".to_string(),
                message: String::new(),
                accounts: Some(result),
            })
        }
    }
}

#[tauri::command]
fn create_instance(
    state: State<AppState>,
    name: String,
    version_id: String,
    loader: String,
    loader_version: Option<String>,
) -> Result<Instance, error::Error> {
    let instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.trim().is_empty() {
            version_id.clone()
        } else {
            name.trim().to_string()
        },
        version_id,
        loader,
        loader_version,
        created_at: now_millis(),
        last_played: None,
        installed: false,
        icon: None,
        ram_mb: None,
        java_path: None,
        jvm_args: None,
    };
    paths::ensure_instance(&instance)?;
    let mut list = state.instances.lock().unwrap();
    list.push(instance.clone());
    paths::save_instances(&list)?;
    Ok(instance)
}

#[tauri::command]
fn delete_instance(state: State<AppState>, id: String) -> Result<(), error::Error> {
    let dir = paths::instance_dir(&id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let mut list = state.instances.lock().unwrap();
    list.retain(|instance| instance.id != id);
    paths::save_instances(&list)?;
    Ok(())
}

#[tauri::command]
async fn fetch_versions(state: State<'_, AppState>) -> Result<Vec<VersionSummary>, error::Error> {
    mojang::fetch_manifest(&state.client).await
}

#[tauri::command]
async fn install_instance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Instance, error::Error> {
    let instance = find_instance(&state, &id)?;
    let concurrency = state.settings.lock().unwrap().concurrency;
    let cancel = download::DownloadControl::new();
    state
        .downloads
        .lock()
        .unwrap()
        .insert(id.clone(), cancel.clone());
    let result = mojang::install(&app, &state.client, &instance, concurrency, Some(cancel)).await;
    state.downloads.lock().unwrap().remove(&id);
    result?;
    let mut list = state.instances.lock().unwrap();
    if let Some(item) = list.iter_mut().find(|item| item.id == id) {
        item.installed = true;
    }
    paths::save_instances(&list)?;
    Ok(list.iter().find(|item| item.id == id).cloned().unwrap())
}

#[tauri::command]
async fn launch_instance(
    state: State<'_, AppState>,
    id: String,
) -> Result<LaunchInfo, error::Error> {
    let instance = find_instance(&state, &id)?;
    if !instance.installed {
        return Err(error::Error::Msg("Сначала установи версию".to_string()));
    }
    let settings = state.settings.lock().unwrap().clone();
    let account = state
        .accounts
        .lock()
        .unwrap()
        .active()
        .cloned()
        .ok_or_else(|| error::Error::Msg("Добавь аккаунт в разделе «Аккаунты»".to_string()))?;
    let mut auth = LaunchAuth {
        name: account.name.clone(),
        uuid: account.uuid.clone(),
        access_token: "0".to_string(),
        user_type: "legacy".to_string(),
        xuid: account.xuid.clone().unwrap_or_else(|| "0".to_string()),
        client_id: "0".to_string(),
    };
    if account.kind == "microsoft" {
        let cached = state
            .microsoft_tokens
            .lock()
            .unwrap()
            .get(&account.id)
            .cloned();
        let token = microsoft::ensure_token(&state.auth_client, &account, cached).await?;
        state
            .microsoft_tokens
            .lock()
            .unwrap()
            .insert(account.id.clone(), token.clone());
        auth.name = token.profile.name;
        auth.uuid = microsoft::dashed_uuid(&token.profile.id);
        auth.access_token = token.access_token;
        auth.user_type = "msa".to_string();
        auth.xuid = token.xuid;
        auth.client_id = microsoft::CLIENT_ID.to_string();
    }
    let java = pick_java(&settings, &instance)?;
    let version = mojang::resolve_version(&state.client, &instance).await?;
    let info = launch::launch(&instance, &version, &settings, &java, &auth)?;
    let mut list = state.instances.lock().unwrap();
    if let Some(item) = list.iter_mut().find(|item| item.id == id) {
        item.last_played = Some(now_millis());
    }
    paths::save_instances(&list)?;
    Ok(info)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct JavaCheck {
    required: u32,
    found: bool,
}

#[tauri::command]
fn check_instance_java(state: State<AppState>, id: String) -> Result<JavaCheck, error::Error> {
    let instance = find_instance(&state, &id)?;
    let settings = state.settings.lock().unwrap().clone();
    let explicit = |path: &Option<String>| {
        path.as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    };
    if explicit(&instance.java_path) || explicit(&settings.java_path) {
        return Ok(JavaCheck {
            required: 0,
            found: true,
        });
    }
    let required = preferred_java_major(&instance.version_id);
    let found = java::detect().iter().any(|item| item.major == required);
    Ok(JavaCheck { required, found })
}

#[tauri::command]
fn detect_java() -> Vec<JavaInfo> {
    java::detect()
}

#[tauri::command]
async fn install_java(state: State<'_, AppState>, major: u32) -> Result<JavaInfo, error::Error> {
    java::install_managed(&state.client, major).await
}

#[tauri::command]
fn remove_java(major: u32) -> Result<(), error::Error> {
    java::remove_managed(major)
}

#[tauri::command]
fn cancel_download(state: State<AppState>, id: String) -> Result<(), error::Error> {
    if let Some(control) = state.downloads.lock().unwrap().get(&id) {
        control.cancel();
    }
    Ok(())
}

#[tauri::command]
fn pause_download(state: State<AppState>, id: String) -> Result<(), error::Error> {
    if let Some(control) = state.downloads.lock().unwrap().get(&id) {
        control.pause();
    }
    Ok(())
}

#[tauri::command]
fn resume_download(state: State<AppState>, id: String) -> Result<(), error::Error> {
    if let Some(control) = state.downloads.lock().unwrap().get(&id) {
        control.resume();
    }
    Ok(())
}

#[tauri::command]
fn clear_cache() -> Result<(), error::Error> {
    let tmp = paths::root().join("tmp");
    if tmp.is_dir() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    Ok(())
}

#[tauri::command]
fn get_data_dir() -> String {
    paths::root().to_string_lossy().to_string()
}

#[tauri::command]
fn get_instance_dir(id: String) -> String {
    paths::instance_dir(&id).to_string_lossy().to_string()
}

#[tauri::command]
fn get_mods_dir(id: String) -> Result<String, error::Error> {
    let dir = paths::instance_dir(&id).join("game").join("mods");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
fn read_log(id: String, lines: Option<usize>) -> Result<String, error::Error> {
    let path = paths::instance_dir(&id).join("logs").join("latest.log");
    if !path.is_file() {
        return Ok(String::new());
    }
    use std::io::{Read, Seek, SeekFrom};
    let limit = lines.unwrap_or(500).clamp(1, 5000);
    let mut file = std::fs::File::open(&path)?;
    let len = file.metadata()?.len();
    let max_bytes = (limit as u64 * 512).max(64 * 1024);
    file.seek(SeekFrom::Start(len.saturating_sub(max_bytes)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(limit);
    Ok(all[start..].join("\n"))
}

#[tauri::command]
fn get_log_file(id: String) -> String {
    paths::instance_dir(&id)
        .join("logs")
        .join("latest.log")
        .to_string_lossy()
        .to_string()
}

#[tauri::command]
async fn fetch_fabric_loaders(
    state: State<'_, AppState>,
    game_version: String,
) -> Result<Vec<model::FabricLoader>, error::Error> {
    fabric::fetch_loaders(&state.client, &game_version).await
}

#[tauri::command]
async fn fetch_forge_versions(
    state: State<'_, AppState>,
    game_version: String,
) -> Result<Vec<model::ForgeVersion>, error::Error> {
    forge::fetch_versions(&state.client, &game_version).await
}

#[tauri::command]
async fn modrinth_search(
    state: State<'_, AppState>,
    query: String,
    game_version: String,
    loader: String,
    project_type: String,
    sort: String,
    offset: u64,
    limit: u64,
) -> Result<model::ModrinthSearchResult, error::Error> {
    modrinth::search(
        &state.client,
        &query,
        &game_version,
        &loader,
        &project_type,
        &sort,
        offset,
        limit,
    )
    .await
}

#[tauri::command]
async fn modrinth_versions(
    state: State<'_, AppState>,
    project_id: String,
    game_version: String,
    loader: String,
) -> Result<Vec<model::ModrinthVersion>, error::Error> {
    modrinth::fetch_versions(&state.client, &project_id, &game_version, &loader).await
}

#[tauri::command]
async fn modrinth_install(
    state: State<'_, AppState>,
    instance_id: String,
    project_id: String,
    game_version: String,
    loader: String,
) -> Result<model::InstalledMod, error::Error> {
    modrinth::install_mod(
        &state.client,
        &instance_id,
        &project_id,
        &game_version,
        &loader,
    )
    .await
}

#[tauri::command]
async fn modrinth_install_modpack(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Instance, error::Error> {
    let concurrency = state.settings.lock().unwrap().concurrency;
    let task_id = format!("modpack:{project_id}");
    let cancel = download::DownloadControl::new();
    state
        .downloads
        .lock()
        .unwrap()
        .insert(task_id.clone(), cancel.clone());
    let result = modrinth::install_modpack(
        &app,
        &state.client,
        &project_id,
        None,
        concurrency,
        Some(cancel),
    )
    .await;
    state.downloads.lock().unwrap().remove(&task_id);
    let mut instance = result?;
    instance.installed = true;
    let mut list = state.instances.lock().unwrap();
    list.push(instance.clone());
    paths::save_instances(&list)?;
    Ok(instance)
}

#[tauri::command]
async fn modrinth_install_version(
    state: State<'_, AppState>,
    instance_id: String,
    version_id: String,
) -> Result<model::InstalledMod, error::Error> {
    let instance = find_instance(&state, &instance_id)?;
    modrinth::install_version(
        &state.client,
        &instance_id,
        &version_id,
        &instance.version_id,
        &instance.loader,
    )
    .await
}

#[tauri::command]
async fn modrinth_install_modpack_version(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    version_id: String,
) -> Result<Instance, error::Error> {
    let concurrency = state.settings.lock().unwrap().concurrency;
    let task_id = format!("modpack:{project_id}");
    let cancel = download::DownloadControl::new();
    state
        .downloads
        .lock()
        .unwrap()
        .insert(task_id.clone(), cancel.clone());
    let result = modrinth::install_modpack(
        &app,
        &state.client,
        &project_id,
        Some(&version_id),
        concurrency,
        Some(cancel),
    )
    .await;
    state.downloads.lock().unwrap().remove(&task_id);
    let mut instance = result?;
    instance.installed = true;
    let mut list = state.instances.lock().unwrap();
    list.push(instance.clone());
    paths::save_instances(&list)?;
    Ok(instance)
}

#[tauri::command]
async fn import_modpack_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    task_id: Option<String>,
) -> Result<Instance, error::Error> {
    let path_ref = std::path::Path::new(&path);
    if !path_ref.is_file() {
        return Err(error::Error::Msg("Файл не найден".to_string()));
    }
    let fallback = path_ref
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Модпак")
        .to_string();
    let concurrency = state.settings.lock().unwrap().concurrency;
    let task_id = task_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("modpack:import:{}", uuid::Uuid::new_v4()));
    let cancel = download::DownloadControl::new();
    state
        .downloads
        .lock()
        .unwrap()
        .insert(task_id.clone(), cancel.clone());
    let result = modrinth::install_mrpack_file(
        &app,
        &state.client,
        path_ref,
        &fallback,
        None,
        "",
        "",
        &fallback,
        concurrency,
        Some(cancel),
    )
    .await;
    state.downloads.lock().unwrap().remove(&task_id);
    let mut instance = result?;
    instance.installed = true;
    let mut list = state.instances.lock().unwrap();
    list.push(instance.clone());
    paths::save_instances(&list)?;
    Ok(instance)
}

#[tauri::command]
fn get_pack_manifest(instance_id: String) -> Option<modrinth::PackManifest> {
    modrinth::read_pack_manifest(&instance_id)
}

#[tauri::command]
fn list_pack_manifests(state: State<AppState>) -> HashMap<String, modrinth::PackManifest> {
    let list = state.instances.lock().unwrap();
    list.iter()
        .filter_map(|instance| {
            modrinth::read_pack_manifest(&instance.id)
                .map(|manifest| (instance.id.clone(), manifest))
        })
        .collect()
}

#[tauri::command]
async fn update_modpack(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    version_id: Option<String>,
) -> Result<Instance, error::Error> {
    let mut instance = find_instance(&state, &id)?;
    let concurrency = state.settings.lock().unwrap().concurrency;
    let cancel = download::DownloadControl::new();
    state
        .downloads
        .lock()
        .unwrap()
        .insert(id.clone(), cancel.clone());
    let result = modrinth::update_modpack(
        &app,
        &state.client,
        &mut instance,
        version_id.as_deref(),
        concurrency,
        Some(cancel),
    )
    .await;
    state.downloads.lock().unwrap().remove(&id);
    result?;
    instance.installed = true;
    let mut list = state.instances.lock().unwrap();
    if let Some(item) = list.iter_mut().find(|item| item.id == instance.id) {
        *item = instance.clone();
    }
    paths::save_instances(&list)?;
    Ok(instance)
}

#[tauri::command]
async fn modrinth_update_all(
    state: State<'_, AppState>,
    instance_id: String,
    game_version: String,
    loader: String,
) -> Result<Vec<model::InstalledMod>, error::Error> {
    modrinth::update_all(&state.client, &instance_id, &game_version, &loader).await
}

#[tauri::command]
async fn modrinth_reindex(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<Vec<model::InstalledMod>, error::Error> {
    modrinth::reindex_mods(&state.client, &instance_id).await
}

#[tauri::command]
fn list_mods(instance_id: String) -> Result<Vec<model::InstalledMod>, error::Error> {
    modrinth::list_mods(&instance_id)
}

#[tauri::command]
fn install_mod_file(
    instance_id: String,
    path: String,
) -> Result<model::InstalledMod, error::Error> {
    modrinth::install_mod_file(&instance_id, &path)
}

#[tauri::command]
fn toggle_mod(instance_id: String, filename: String) -> Result<bool, error::Error> {
    modrinth::toggle_mod(&instance_id, &filename)
}

#[tauri::command]
fn delete_mod(instance_id: String, filename: String) -> Result<(), error::Error> {
    modrinth::delete_mod(&instance_id, &filename)
}

#[tauri::command]
async fn instance_stats(instance_id: String) -> Result<model::InstanceStats, error::Error> {
    let root = paths::instance_dir(&instance_id);
    let game = root.join("game");
    let mods = std::fs::read_dir(game.join("mods"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().is_file() && entry.file_name().to_string_lossy().ends_with(".jar")
                })
                .count() as u32
        })
        .unwrap_or(0);
    let worlds = std::fs::read_dir(game.join("saves"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .count() as u32
        })
        .unwrap_or(0);
    Ok(model::InstanceStats {
        size_bytes: dir_size(&root),
        mods,
        worlds,
    })
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

fn unique_name(list: &[Instance], base: &str) -> String {
    if !list.iter().any(|instance| instance.name == base) {
        return base.to_string();
    }
    let mut counter = 2;
    loop {
        let candidate = format!("{base} {counter}");
        if !list.iter().any(|instance| instance.name == candidate) {
            return candidate;
        }
        counter += 1;
    }
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> Result<(), error::Error> {
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

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &std::path::Path,
    dir: &std::path::Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), error::Error> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| error::Error::Msg("Не удалось собрать путь в архиве".to_string()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            zip.add_directory(format!("{name}/"), options)?;
            add_dir_to_zip(zip, root, &path, options)?;
        } else if file_type.is_file() {
            zip.start_file(name, options)?;
            let mut source = std::fs::File::open(&path)?;
            std::io::copy(&mut source, zip)?;
        }
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim_matches('_').is_empty() {
        "instance".to_string()
    } else {
        cleaned
    }
}

#[tauri::command]
fn rename_instance(
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<Instance, error::Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(error::Error::Msg("Имя не может быть пустым".to_string()));
    }
    let mut list = state.instances.lock().unwrap();
    let Some(instance) = list.iter_mut().find(|instance| instance.id == id) else {
        return Err(error::Error::Msg("Инстанс не найден".to_string()));
    };
    instance.name = name.to_string();
    let updated = instance.clone();
    paths::save_instances(&list)?;
    Ok(updated)
}

#[tauri::command]
async fn duplicate_instance(
    state: State<'_, AppState>,
    id: String,
) -> Result<Instance, error::Error> {
    let source = find_instance(&state, &id)?;
    let name = {
        let list = state.instances.lock().unwrap();
        unique_name(&list, &format!("{} (копия)", source.name))
    };
    let mut copy = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        version_id: source.version_id.clone(),
        loader: source.loader.clone(),
        loader_version: source.loader_version.clone(),
        created_at: now_millis(),
        last_played: None,
        installed: source.installed,
        icon: None,
        ram_mb: source.ram_mb,
        java_path: source.java_path.clone(),
        jvm_args: source.jvm_args.clone(),
    };
    copy_dir(
        &paths::instance_dir(&source.id),
        &paths::instance_dir(&copy.id),
    )?;
    copy.icon = local_instance_icon(&paths::instance_dir(&copy.id))
        .or_else(|| source.icon.clone().filter(|icon| icon.starts_with("http")));
    let mut list = state.instances.lock().unwrap();
    list.push(copy.clone());
    paths::save_instances(&list)?;
    Ok(copy)
}

fn write_instance_zip(
    root: &std::path::Path,
    instance: &Instance,
    path: &std::path::Path,
) -> Result<(), error::Error> {
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("instance.json", options)?;
    let manifest = serde_json::to_vec_pretty(instance)?;
    std::io::Write::write_all(&mut zip, &manifest)?;
    add_dir_to_zip(&mut zip, root, root, options)?;
    zip.finish()?;
    Ok(())
}

fn write_game_zip(root: &std::path::Path, path: &std::path::Path) -> Result<(), error::Error> {
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_game_entry(&name) {
            continue;
        }
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            zip.add_directory(format!("{name}/"), options)?;
            add_dir_to_zip(&mut zip, root, &entry_path, options)?;
        } else if file_type.is_file() {
            zip.start_file(name, options)?;
            let mut source = std::fs::File::open(&entry_path)?;
            std::io::copy(&mut source, &mut zip)?;
        }
    }
    zip.finish()?;
    Ok(())
}

fn should_skip_game_entry(name: &str) -> bool {
    matches!(
        name,
        "logs"
            | "crash-reports"
            | "saves"
            | "screenshots"
            | "backups"
            | ".mixin.out"
            | "debug"
            | "usercache.json"
            | "usernamecache.json"
            | "launcher_profiles.json"
            | "servers.dat"
            | "realms_persistence.json"
    )
}

fn extract_instance_zip(
    path: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), error::Error> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    std::fs::create_dir_all(target.join("game"))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let name_text = relative.to_string_lossy().replace('\\', "/");
        if name_text == "instance.json" {
            continue;
        }
        let out = target.join(&name_text);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut data)?;
        std::fs::write(&out, &data)?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum ImportKind {
    OneLauncher,
    Prism,
}

struct ImportManifest {
    kind: ImportKind,
    name: String,
    version_id: String,
    loader: String,
    loader_version: Option<String>,
    installed: bool,
    icon: Option<String>,
    ram_mb: Option<u32>,
    java_path: Option<String>,
    jvm_args: Option<String>,
}

#[derive(serde::Deserialize)]
struct MmcPack {
    #[serde(default)]
    components: Vec<MmcComponent>,
}

#[derive(serde::Deserialize)]
struct MmcComponent {
    uid: String,
    #[serde(default)]
    version: String,
}

fn zip_stem(path: &std::path::Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "Импорт".to_string())
}

fn local_instance_icon(root: &std::path::Path) -> Option<String> {
    for candidate in [root.join("icon.png"), root.join("game").join("icon.png")] {
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

fn ini_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let value = rest.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn parse_mmc_pack(text: &str) -> Result<(String, String, Option<String>), error::Error> {
    let pack: MmcPack = serde_json::from_str(text)?;
    let mut version_id = None;
    let mut loader = "vanilla".to_string();
    let mut loader_version = None;
    for component in pack.components {
        match component.uid.as_str() {
            "net.minecraft" => version_id = Some(component.version),
            "net.fabricmc.fabric-loader" => {
                loader = "fabric".to_string();
                loader_version = Some(component.version);
            }
            "org.quiltmc.quilt-loader" => {
                return Err(error::Error::Msg(
                    "Quilt-инстансы пока не поддерживаются".to_string(),
                ));
            }
            "net.minecraftforge" => {
                return Err(error::Error::Msg(
                    "Forge-инстансы пока не поддерживаются".to_string(),
                ));
            }
            "net.neoforged" => {
                return Err(error::Error::Msg(
                    "NeoForge-инстансы пока не поддерживаются".to_string(),
                ));
            }
            _ => {}
        }
    }
    let version_id = version_id
        .filter(|version| !version.trim().is_empty())
        .ok_or_else(|| {
            error::Error::Msg("В mmc-pack.json не указана версия Minecraft".to_string())
        })?;
    Ok((version_id, loader, loader_version))
}

fn read_import_manifest(path: &std::path::Path) -> Result<ImportManifest, error::Error> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    if let Ok(mut manifest) = archive.by_name("instance.json") {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut manifest, &mut text)?;
        let instance: Instance = serde_json::from_str(&text)?;
        return Ok(ImportManifest {
            kind: ImportKind::OneLauncher,
            name: instance.name,
            version_id: instance.version_id,
            loader: instance.loader,
            loader_version: instance.loader_version,
            installed: instance.installed,
            icon: instance.icon,
            ram_mb: instance.ram_mb,
            java_path: instance.java_path,
            jvm_args: instance.jvm_args,
        });
    }
    let mut config = String::new();
    if let Ok(mut entry) = archive.by_name("instance.cfg") {
        std::io::Read::read_to_string(&mut entry, &mut config)?;
    }
    let mut pack_text = String::new();
    if let Ok(mut entry) = archive.by_name("mmc-pack.json") {
        std::io::Read::read_to_string(&mut entry, &mut pack_text)?;
    }
    if !pack_text.trim().is_empty() {
        let (version_id, loader, loader_version) = parse_mmc_pack(&pack_text)?;
        let name = ini_value(&config, "name").unwrap_or_else(|| zip_stem(path));
        return Ok(ImportManifest {
            kind: ImportKind::Prism,
            name,
            version_id,
            loader,
            loader_version,
            installed: false,
            icon: None,
            ram_mb: None,
            java_path: None,
            jvm_args: None,
        });
    }
    Err(error::Error::Msg(
        "Не удалось распознать архив. Поддерживаются экспорт Bubanya/OneLauncher (instance.json) и Prism/MultiMC (instance.cfg + mmc-pack.json).".to_string(),
    ))
}

fn extract_prism_zip(path: &std::path::Path, target: &std::path::Path) -> Result<(), error::Error> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let game = target.join("game");
    std::fs::create_dir_all(&game)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let name_text = relative.to_string_lossy().replace('\\', "/");
        if name_text == "icon.png" {
            let out = target.join("icon.png");
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut data)?;
            std::fs::write(&out, &data)?;
            continue;
        }
        let Some(rest) = name_text
            .strip_prefix("minecraft/")
            .or_else(|| name_text.strip_prefix(".minecraft/"))
        else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let Some(relative) = crate::modrinth::safe_relative(rest) else {
            continue;
        };
        let out = game.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut data)?;
        std::fs::write(&out, &data)?;
    }
    Ok(())
}

fn parse_toml_pair(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    let value = rest.trim_matches(|character| character == '\'' || character == '"');
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn toml_top_value(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            break;
        }
        if let Some(value) = parse_toml_pair(line, key) {
            return Some(value);
        }
    }
    None
}

fn toml_section_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == section;
            continue;
        }
        if in_section && let Some(value) = parse_toml_pair(line, key) {
            return Some(value);
        }
    }
    None
}

fn build_mod_index(mods_dir: &std::path::Path) -> Result<(), error::Error> {
    let index_dir = mods_dir.join(".index");
    let Ok(entries) = std::fs::read_dir(&index_dir) else {
        return Ok(());
    };
    let mut index = crate::modrinth::ModIndex::default();
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .map(|extension| extension != "toml")
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(filename) = toml_top_value(&text, "filename") else {
            continue;
        };
        let Some(project_id) = toml_section_value(&text, "[update.modrinth]", "mod-id") else {
            continue;
        };
        let title = toml_top_value(&text, "name").unwrap_or_else(|| filename.clone());
        let version = toml_top_value(&text, "x-prismlauncher-version-number")
            .or_else(|| toml_section_value(&text, "[update.modrinth]", "version"))
            .unwrap_or_default();
        index.entries.insert(
            filename,
            crate::modrinth::IndexEntry {
                project_id,
                title,
                version,
            },
        );
    }
    if !index.entries.is_empty() {
        crate::modrinth::write_index(mods_dir, &index)?;
    }
    Ok(())
}

#[tauri::command]
async fn export_instance(state: State<'_, AppState>, id: String) -> Result<String, error::Error> {
    let instance = find_instance(&state, &id)?;
    let exports = paths::root().join("exports");
    std::fs::create_dir_all(&exports)?;
    let path = exports.join(format!(
        "{}-{}.zip",
        sanitize_filename(&instance.name),
        now_millis()
    ));
    write_instance_zip(&paths::instance_dir(&id), &instance, &path)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn export_instance_game(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, error::Error> {
    let instance = find_instance(&state, &id)?;
    let game = paths::instance_dir(&id).join("game");
    if !game.is_dir() {
        return Err(error::Error::Msg("У инстанса нет папки game".to_string()));
    }
    let exports = paths::root().join("exports");
    std::fs::create_dir_all(&exports)?;
    let path = exports.join(format!(
        "{}-game-{}.zip",
        sanitize_filename(&instance.name),
        now_millis()
    ));
    write_game_zip(&game, &path)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
async fn import_instance(
    state: State<'_, AppState>,
    path: String,
) -> Result<Instance, error::Error> {
    let path_ref = std::path::Path::new(&path);
    let source = read_import_manifest(path_ref)?;
    let base_name = if source.name.trim().is_empty() {
        zip_stem(path_ref)
    } else {
        source.name.clone()
    };
    let name = {
        let list = state.instances.lock().unwrap();
        unique_name(&list, &base_name)
    };
    let mut instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        version_id: source.version_id,
        loader: source.loader,
        loader_version: source.loader_version,
        created_at: now_millis(),
        last_played: None,
        installed: source.installed,
        icon: source.icon.clone(),
        ram_mb: source.ram_mb,
        java_path: source.java_path.clone(),
        jvm_args: source.jvm_args.clone(),
    };
    let target = paths::instance_dir(&instance.id);
    match source.kind {
        ImportKind::OneLauncher => extract_instance_zip(path_ref, &target)?,
        ImportKind::Prism => extract_prism_zip(path_ref, &target)?,
    }
    instance.icon = local_instance_icon(&target)
        .or_else(|| source.icon.filter(|icon| icon.starts_with("http")));
    build_mod_index(&target.join("game").join("mods"))?;
    let mut list = state.instances.lock().unwrap();
    list.push(instance.clone());
    paths::save_instances(&list)?;
    Ok(instance)
}

#[tauri::command]
fn list_content(
    instance_id: String,
    kind: String,
) -> Result<Vec<model::ContentFile>, error::Error> {
    content::list_packs(&instance_id, &kind)
}

#[tauri::command]
fn toggle_content(
    instance_id: String,
    kind: String,
    filename: String,
) -> Result<bool, error::Error> {
    content::toggle_pack(&instance_id, &kind, &filename)
}

#[tauri::command]
fn delete_content(instance_id: String, kind: String, filename: String) -> Result<(), error::Error> {
    content::delete_pack(&instance_id, &kind, &filename)
}

#[tauri::command]
fn import_content(
    instance_id: String,
    kind: String,
    path: String,
) -> Result<model::ContentFile, error::Error> {
    content::import_pack(&instance_id, &kind, &path)
}

#[tauri::command]
fn get_content_dir(instance_id: String, kind: String) -> Result<String, error::Error> {
    content::pack_dir_for_command(&instance_id, &kind)
}

#[tauri::command]
fn list_worlds(instance_id: String) -> Result<Vec<model::WorldInfo>, error::Error> {
    content::list_worlds(&instance_id)
}

#[tauri::command]
fn delete_world(instance_id: String, folder: String) -> Result<(), error::Error> {
    content::delete_world(&instance_id, &folder)
}

#[tauri::command]
fn backup_world(instance_id: String, folder: String) -> Result<String, error::Error> {
    content::backup_world(&instance_id, &folder)
}

#[tauri::command]
fn import_world(instance_id: String, path: String) -> Result<model::WorldInfo, error::Error> {
    content::import_world(&instance_id, &path)
}

#[tauri::command]
fn get_worlds_dir(instance_id: String) -> Result<String, error::Error> {
    content::worlds_dir_for_command(&instance_id)
}

#[tauri::command]
fn update_instance_settings(
    state: State<AppState>,
    id: String,
    name: String,
    ram_mb: Option<u32>,
    java_path: Option<String>,
    jvm_args: Option<String>,
) -> Result<Instance, error::Error> {
    let name = name.trim();
    if name.is_empty() {
        return Err(error::Error::Msg("Имя не может быть пустым".to_string()));
    }
    if let Some(ram) = ram_mb
        && !(512..=32768).contains(&ram)
    {
        return Err(error::Error::Msg(
            "Память должна быть от 512 до 32768 МБ".to_string(),
        ));
    }
    let mut list = state.instances.lock().unwrap();
    let Some(instance) = list.iter_mut().find(|instance| instance.id == id) else {
        return Err(error::Error::Msg("Инстанс не найден".to_string()));
    };
    instance.name = name.to_string();
    instance.ram_mb = ram_mb;
    instance.java_path = java_path.filter(|path| !path.trim().is_empty());
    instance.jvm_args = jvm_args.filter(|args| !args.trim().is_empty());
    let updated = instance.clone();
    paths::save_instances(&list)?;
    Ok(updated)
}

fn find_instance(state: &State<AppState>, id: &str) -> Result<Instance, error::Error> {
    state
        .instances
        .lock()
        .unwrap()
        .iter()
        .find(|instance| instance.id == id)
        .cloned()
        .ok_or_else(|| error::Error::Msg("Инстанс не найден".to_string()))
}

fn pick_java(settings: &Settings, instance: &Instance) -> Result<JavaInfo, error::Error> {
    if let Some(path) = &instance.java_path
        && !path.trim().is_empty()
    {
        return Ok(JavaInfo {
            path: path.clone(),
            version: String::new(),
            major: 0,
        });
    }
    if let Some(path) = &settings.java_path
        && !path.trim().is_empty()
    {
        return Ok(JavaInfo {
            path: path.clone(),
            version: String::new(),
            major: 0,
        });
    }
    let javas = java::detect();
    let preferred = preferred_java_major(&instance.version_id);
    if let Some(found) = javas.iter().find(|java| java.major == preferred) {
        return Ok(found.clone());
    }
    javas
        .into_iter()
        .next()
        .ok_or_else(|| error::Error::Msg("Java не найдена. Укажи путь в настройках.".to_string()))
}

pub(crate) fn preferred_java_major(version_id: &str) -> u32 {
    let mut parts = version_id.split('.');
    let major = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    if major >= 26 {
        return 25;
    }
    if major != 1 {
        return 21;
    }
    if minor <= 16 {
        8
    } else if minor == 17 {
        16
    } else if minor < 20 || (minor == 20 && patch <= 4) {
        17
    } else {
        21
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    paths::ensure().ok();
    let state = AppState {
        client: reqwest::Client::builder()
            .user_agent("Bubanya/0.1")
            .build()
            .expect("http client"),
        auth_client: reqwest::Client::builder()
            .user_agent("Bubanya/0.1")
            .http1_only()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("auth http client"),
        settings: Mutex::new(paths::load_settings()),
        instances: Mutex::new(paths::load_instances()),
        accounts: Mutex::new(paths::load_accounts()),
        microsoft_tokens: Mutex::new(HashMap::new()),
        downloads: Mutex::new(HashMap::new()),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            list_instances,
            list_accounts,
            update_instance_settings,
            add_offline_account,
            rename_account,
            delete_account,
            set_active_account,
            get_account_skin,
            microsoft_begin_login,
            microsoft_poll_login,
            create_instance,
            delete_instance,
            fetch_versions,
            install_instance,
            launch_instance,
            detect_java,
            check_instance_java,
            install_java,
            remove_java,
            cancel_download,
            pause_download,
            resume_download,
            get_data_dir,
            clear_cache,
            get_instance_dir,
            get_mods_dir,
            get_log_file,
            read_log,
            fetch_fabric_loaders,
            fetch_forge_versions,
            modrinth_search,
            modrinth_versions,
            modrinth_install,
            modrinth_install_modpack,
            modrinth_install_version,
            modrinth_install_modpack_version,
            import_modpack_file,
            get_pack_manifest,
            list_pack_manifests,
            update_modpack,
            modrinth_update_all,
            modrinth_reindex,
            list_mods,
            install_mod_file,
            list_content,
            toggle_content,
            delete_content,
            import_content,
            get_content_dir,
            list_worlds,
            delete_world,
            backup_world,
            import_world,
            get_worlds_dir,
            toggle_mod,
            delete_mod,
            instance_stats,
            rename_instance,
            duplicate_instance,
            export_instance,
            export_instance_game,
            import_instance,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bubanya-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn export_import_roundtrip() {
        let base = temp_dir();
        let source = base.join("source");
        std::fs::create_dir_all(source.join("game/mods")).unwrap();
        std::fs::create_dir_all(source.join("game/saves/world")).unwrap();
        std::fs::write(source.join("game/mods/example.jar"), b"jar-bytes").unwrap();
        std::fs::write(source.join("game/saves/world/level.dat"), b"level-bytes").unwrap();
        let instance = Instance {
            id: "source-id".to_string(),
            name: "Test / Instance".to_string(),
            version_id: "26.3".to_string(),
            loader: "fabric".to_string(),
            loader_version: Some("0.20.1".to_string()),
            created_at: 1,
            last_played: None,
            installed: true,
            icon: None,
            ram_mb: None,
            java_path: None,
            jvm_args: None,
        };
        assert_eq!(dir_size(&source), 20);
        let zip_path = base.join("export.zip");
        write_instance_zip(&source, &instance, &zip_path).unwrap();
        let manifest = read_import_manifest(&zip_path).unwrap();
        assert!(matches!(manifest.kind, ImportKind::OneLauncher));
        assert_eq!(manifest.name, instance.name);
        assert_eq!(manifest.version_id, "26.3");
        assert_eq!(manifest.loader, "fabric");
        let target = base.join("target");
        extract_instance_zip(&zip_path, &target).unwrap();
        assert_eq!(
            std::fs::read(target.join("game/mods/example.jar")).unwrap(),
            b"jar-bytes"
        );
        assert_eq!(
            std::fs::read(target.join("game/saves/world/level.dat")).unwrap(),
            b"level-bytes"
        );
        assert!(!target.join("instance.json").exists());

        let copy = base.join("copy");
        copy_dir(&source, &copy).unwrap();
        assert_eq!(dir_size(&copy), dir_size(&source));

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn prism_import_roundtrip() {
        let base = temp_dir();
        let zip_path = base.join("JungarickWorld.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("instance.cfg", options).unwrap();
        std::io::Write::write_all(&mut zip, b"[General]\nname=JungarickWorld\n").unwrap();
        zip.start_file("mmc-pack.json", options).unwrap();
        std::io::Write::write_all(
            &mut zip,
            br#"{"components":[{"uid":"net.minecraft","version":"26.3"},{"uid":"net.fabricmc.fabric-loader","version":"0.20.1"}]}"#,
        )
        .unwrap();
        zip.start_file("minecraft/mods/.index/sodium.pw.toml", options)
            .unwrap();
        std::io::Write::write_all(
            &mut zip,
            b"filename = 'sodium.jar'\nname = 'Sodium'\nx-prismlauncher-version-number = 'mc26.3-1.0'\n[update.modrinth]\nmod-id = 'AANobbMI'\nversion = 'g6ju16VJ'\n",
        )
        .unwrap();
        zip.start_file("minecraft/mods/sodium.jar", options)
            .unwrap();
        std::io::Write::write_all(&mut zip, b"jar").unwrap();
        zip.finish().unwrap();

        let manifest = read_import_manifest(&zip_path).unwrap();
        assert!(matches!(manifest.kind, ImportKind::Prism));
        assert_eq!(manifest.name, "JungarickWorld");
        assert_eq!(manifest.version_id, "26.3");
        assert_eq!(manifest.loader, "fabric");
        assert_eq!(manifest.loader_version.as_deref(), Some("0.20.1"));
        assert!(!manifest.installed);

        let target = base.join("target");
        extract_prism_zip(&zip_path, &target).unwrap();
        assert!(target.join("game/mods/sodium.jar").exists());
        build_mod_index(&target.join("game/mods")).unwrap();
        let index = crate::modrinth::read_index(&target.join("game/mods"));
        let entry = index.entries.get("sodium.jar").unwrap();
        assert_eq!(entry.project_id, "AANobbMI");
        assert_eq!(entry.title, "Sodium");
        assert_eq!(entry.version, "mc26.3-1.0");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn game_zip_keeps_game_contents_at_root() {
        let base = temp_dir();
        let game = base.join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::create_dir_all(game.join("saves/world")).unwrap();
        std::fs::create_dir_all(game.join("logs")).unwrap();
        std::fs::write(game.join("mods/example.jar"), b"jar").unwrap();
        std::fs::write(game.join("config/example.toml"), b"config").unwrap();
        std::fs::write(game.join("saves/world/level.dat"), b"level").unwrap();
        std::fs::write(game.join("logs/latest.log"), b"log").unwrap();
        let zip_path = base.join("game.zip");
        write_game_zip(&game, &zip_path).unwrap();
        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"mods/example.jar".to_string()));
        assert!(names.contains(&"config/example.toml".to_string()));
        assert!(!names.iter().any(|name| name.starts_with("game/")));
        assert!(!names.iter().any(|name| name.starts_with("saves/")));
        assert!(!names.iter().any(|name| name.starts_with("logs/")));
        assert!(!names.contains(&"instance.json".to_string()));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn nickname_validation() {
        assert!(validate_nickname("Steve_1").is_ok());
        assert!(validate_nickname("ab").is_err());
        assert!(validate_nickname("ник").is_err());
        assert!(validate_nickname("bad name").is_err());
    }

    #[test]
    fn accounts_active_lookup() {
        let account = Account {
            id: "a".to_string(),
            kind: "offline".to_string(),
            name: "Steve".to_string(),
            uuid: "u".to_string(),
            xuid: None,
            created_at: 0,
        };
        let accounts = Accounts {
            accounts: vec![account.clone()],
            active_id: Some("a".to_string()),
        };
        assert_eq!(
            accounts.active().map(|item| item.name.as_str()),
            Some("Steve")
        );
    }

    #[test]
    fn unique_name_appends_counter() {
        let make = |name: &str| Instance {
            id: name.to_string(),
            name: name.to_string(),
            version_id: "26.3".to_string(),
            loader: "vanilla".to_string(),
            loader_version: None,
            created_at: 0,
            last_played: None,
            installed: false,
            icon: None,
            ram_mb: None,
            java_path: None,
            jvm_args: None,
        };
        let list = vec![make("Мир"), make("Мир 2")];
        assert_eq!(unique_name(&list, "Мир"), "Мир 3");
        assert_eq!(unique_name(&list, "Новый"), "Новый");
    }
}
