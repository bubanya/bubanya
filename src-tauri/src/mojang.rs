use crate::download::DownloadControl;
use crate::error::{Error, Result};
use crate::model::{Instance, Progress, VersionSummary};
use crate::paths;
use futures_util::{StreamExt, pin_mut};
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub versions: Vec<ManifestVersion>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub assets: String,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: Downloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Downloads {
    pub client: Option<DownloadRef>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DownloadRef {
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
}

pub async fn fetch_manifest(client: &reqwest::Client) -> Result<Vec<VersionSummary>> {
    let path = paths::meta().join("version_manifest_v2.json");
    let text = match client.get(MANIFEST_URL).send().await {
        Ok(response) => match response.error_for_status() {
            Ok(response) => {
                let text = response.text().await?;
                std::fs::create_dir_all(paths::meta())?;
                std::fs::write(&path, &text)?;
                text
            }
            Err(error) => fallback(&path, error)?,
        },
        Err(error) => fallback(&path, error)?,
    };
    let manifest: Manifest = serde_json::from_str(&text)?;
    Ok(manifest
        .versions
        .into_iter()
        .map(|v| VersionSummary {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
            url: v.url,
        })
        .collect())
}

fn fallback(path: &Path, error: reqwest::Error) -> Result<String> {
    if path.exists() {
        Ok(std::fs::read_to_string(path)?)
    } else {
        Err(error.into())
    }
}

pub async fn fetch_version(client: &reqwest::Client, url: &str, id: &str) -> Result<VersionJson> {
    let path = paths::meta().join("versions").join(format!("{id}.json"));
    if path.exists()
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(version) = serde_json::from_str(&text)
    {
        return Ok(version);
    }
    let text = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &text)?;
    Ok(serde_json::from_str(&text)?)
}

pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: Option<&str>,
) -> Result<()> {
    if dest.exists() {
        match expected {
            Some(expected) => {
                if sha1_of(dest).await.ok().as_deref() == Some(expected) {
                    return Ok(());
                }
            }
            None => return Ok(()),
        }
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut last_error = None;
    for attempt in 1..=3u32 {
        match download_once(client, url, dest, expected).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < 3 {
                    tokio::time::sleep(Duration::from_millis(400 * u64::from(attempt))).await;
                }
            }
        }
    }
    Err(Error::Msg(format!(
        "Не удалось скачать {url}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "неизвестная ошибка".to_string())
    )))
}

async fn download_once(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: Option<&str>,
) -> Result<()> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut part_name = dest.file_name().unwrap_or_default().to_os_string();
    part_name.push(".part");
    let part = dest.with_file_name(part_name);
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha1::new();
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let Some(expected) = expected {
        let actual = hex::encode(hasher.finalize());
        if actual != expected {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::Msg(format!("SHA1 не совпал: {url}")));
        }
    }
    let _ = tokio::fs::remove_file(dest).await;
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

async fn sha1_of(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Ванильная версия или ванильная + Fabric-профиль (слитые библиотеки и аргументы).
pub async fn resolve_version(client: &reqwest::Client, instance: &Instance) -> Result<VersionJson> {
    let manifest = fetch_manifest(client).await?;
    let summary = manifest
        .into_iter()
        .find(|version| version.id == instance.version_id)
        .ok_or_else(|| Error::Msg(format!("Версия {} не найдена", instance.version_id)))?;
    let vanilla = fetch_version(client, &summary.url, &instance.version_id).await?;
    if instance.loader.eq_ignore_ascii_case("forge") {
        let profile = crate::forge::profile(instance)?;
        return Ok(crate::forge::merge(vanilla, profile));
    }
    if !instance.loader.eq_ignore_ascii_case("fabric") {
        return Ok(vanilla);
    }
    let loader = match &instance.loader_version {
        Some(version) if !version.is_empty() => version.clone(),
        _ => crate::fabric::latest_loader(client, &instance.version_id).await?,
    };
    let profile = crate::fabric::fetch_profile(client, &instance.version_id, &loader).await?;
    Ok(crate::fabric::merge(vanilla, profile))
}

pub async fn install(
    app: &AppHandle,
    client: &reqwest::Client,
    instance: &Instance,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<()> {
    emit(app, &instance.id, "prepare", 0, 1, "Манифест версии");
    let is_forge = instance.loader.eq_ignore_ascii_case("forge");
    if is_forge {
        emit(app, &instance.id, "forge", 0, 1, "Установка Forge");
        crate::forge::ensure_installed(client, instance).await?;
        emit(app, &instance.id, "forge", 1, 1, "Forge установлен");
    }
    let version = resolve_version(client, instance).await?;

    let version_dir = paths::meta().join("versions").join(&instance.version_id);
    std::fs::create_dir_all(&version_dir)?;
    if !is_forge && let Some(client_dl) = &version.downloads.client {
        emit(app, &instance.id, "client", 0, 1, "Клиент игры");
        download(
            client,
            &client_dl.url,
            &version_dir.join("client.jar"),
            client_dl.sha1.as_deref(),
        )
        .await?;
    }

    install_libraries(
        app,
        client,
        &version,
        &instance.id,
        concurrency,
        cancel.clone(),
    )
    .await?;
    install_assets(
        app,
        client,
        &version,
        &instance.id,
        concurrency,
        cancel.clone(),
    )
    .await?;
    emit(app, &instance.id, "natives", 0, 1, "Natives");
    extract_natives(&version, &instance.id)?;
    emit(app, &instance.id, "done", 1, 1, "Готово");
    Ok(())
}

async fn install_libraries(
    app: &AppHandle,
    client: &reqwest::Client,
    version: &VersionJson,
    instance_id: &str,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<()> {
    let lib_dir = paths::meta().join("libraries");
    let mut tasks = Vec::new();
    for library in &version.libraries {
        if !rules_allow(&library.rules) {
            continue;
        }
        if is_native(library) {
            if let Some(native) = native_artifact(library)
                && let Some(path) = &native.path
            {
                tasks.push((native.url.clone(), lib_dir.join(path), native.sha1.clone()));
            }
        } else if let Some((url, path, sha1)) = library_file(library) {
            tasks.push((url, lib_dir.join(path), sha1));
        }
    }
    emit(
        app,
        instance_id,
        "libraries",
        0,
        tasks.len() as u64,
        "Библиотеки",
    );
    download_many(
        app,
        client,
        instance_id,
        "libraries",
        tasks,
        concurrency,
        cancel,
    )
    .await
}

async fn install_assets(
    app: &AppHandle,
    client: &reqwest::Client,
    version: &VersionJson,
    instance_id: &str,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<()> {
    let Some(index_ref) = &version.asset_index else {
        return Ok(());
    };
    let index_dir = paths::meta().join("assets").join("indexes");
    std::fs::create_dir_all(&index_dir)?;
    let index_path = index_dir.join(format!("{}.json", index_ref.id));
    download(
        client,
        &index_ref.url,
        &index_path,
        index_ref.sha1.as_deref(),
    )
    .await?;
    let index: AssetIndex = serde_json::from_str(&std::fs::read_to_string(&index_path)?)?;
    let objects_dir = paths::meta().join("assets").join("objects");
    let mut tasks = Vec::with_capacity(index.objects.len());
    for object in index.objects.into_values() {
        let prefix = object.hash.get(0..2).unwrap_or("00").to_string();
        let url = format!("{RESOURCES_URL}/{prefix}/{}", object.hash);
        let dest = objects_dir.join(&prefix).join(&object.hash);
        tasks.push((url, dest, Some(object.hash)));
    }
    emit(app, instance_id, "assets", 0, tasks.len() as u64, "Ресурсы");
    download_many(
        app,
        client,
        instance_id,
        "assets",
        tasks,
        concurrency,
        cancel,
    )
    .await
}

pub async fn download_many(
    app: &AppHandle,
    client: &reqwest::Client,
    instance_id: &str,
    stage: &str,
    tasks: Vec<(String, PathBuf, Option<String>)>,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<()> {
    let total = tasks.len() as u64;
    if total == 0 {
        return Ok(());
    }
    let is_cancelled = || {
        cancel
            .as_ref()
            .is_some_and(|control| control.is_cancelled())
    };
    if is_cancelled() {
        return Err(Error::Msg("Загрузка отменена".to_string()));
    }
    let counter = Arc::new(AtomicU64::new(0));
    let stream = futures_util::stream::iter(tasks.into_iter().map(|(url, dest, sha)| {
        let client = client.clone();
        let counter = Arc::clone(&counter);
        let cancel = cancel.clone();
        async move {
            if cancel
                .as_ref()
                .is_some_and(|control| control.is_cancelled())
            {
                return Err(Error::Msg("Загрузка отменена".to_string()));
            }
            download(&client, &url, &dest, sha.as_deref()).await?;
            Ok::<u64, Error>(counter.fetch_add(1, Ordering::Relaxed) + 1)
        }
    }))
    .buffer_unordered(concurrency.max(1) as usize);
    pin_mut!(stream);
    loop {
        wait_if_paused(&cancel).await?;
        let Some(result) = next_or_pause(stream.as_mut(), &cancel).await? else {
            break;
        };
        if is_cancelled() {
            return Err(Error::Msg("Загрузка отменена".to_string()));
        }
        let done = result?;
        if done % 16 == 0 || done == total {
            emit(app, instance_id, stage, done, total, "");
        }
    }
    Ok(())
}

async fn next_or_pause<S>(
    mut stream: std::pin::Pin<&mut S>,
    control: &Option<Arc<DownloadControl>>,
) -> Result<Option<S::Item>>
where
    S: futures_util::Stream + Unpin,
{
    let Some(control) = control else {
        return Ok(stream.next().await);
    };
    loop {
        if control.is_cancelled() {
            return Err(Error::Msg("Загрузка отменена".to_string()));
        }
        if control.is_paused() {
            tokio::time::sleep(Duration::from_millis(200)).await;
            continue;
        }
        tokio::select! {
            item = stream.next() => return Ok(item),
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
    }
}

async fn wait_if_paused(control: &Option<Arc<DownloadControl>>) -> Result<()> {
    let Some(control) = control else {
        return Ok(());
    };
    while control.is_paused() {
        if control.is_cancelled() {
            return Err(Error::Msg("Загрузка отменена".to_string()));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if control.is_cancelled() {
        return Err(Error::Msg("Загрузка отменена".to_string()));
    }
    Ok(())
}

fn emit(app: &AppHandle, instance_id: &str, stage: &str, current: u64, total: u64, message: &str) {
    let _ = app.emit(
        "install-progress",
        Progress {
            instance_id: instance_id.to_string(),
            stage: stage.to_string(),
            current,
            total,
            message: message.to_string(),
        },
    );
}

pub fn extract_natives(version: &VersionJson, instance_id: &str) -> Result<()> {
    let lib_dir = paths::meta().join("libraries");
    let natives_root = paths::instances().join(instance_id).join("natives");
    // Новые версии ждут нативы в natives/java, старые — в корне natives.
    let targets = [natives_root.clone(), natives_root.join("java")];
    for target in &targets {
        std::fs::create_dir_all(target)?;
    }
    for sub in ["lwjgl", "jna", "netty"] {
        std::fs::create_dir_all(natives_root.join(sub))?;
    }
    for library in &version.libraries {
        if !rules_allow(&library.rules) || !is_native(library) {
            continue;
        }
        let Some(artifact) = native_artifact(library) else {
            continue;
        };
        let Some(path) = artifact.path else {
            continue;
        };
        let jar = lib_dir.join(path);
        if !jar.exists() {
            continue;
        }
        let file = std::fs::File::open(&jar)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            if entry.is_dir() {
                continue;
            }
            let Some(name) = entry.enclosed_name() else {
                continue;
            };
            let file_name = match name.file_name().and_then(|value| value.to_str()) {
                Some(value) => value.to_string(),
                None => continue,
            };
            if !is_native_file(&file_name) {
                continue;
            }
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes)?;
            for target in &targets {
                std::fs::write(target.join(&file_name), &bytes)?;
            }
        }
    }
    Ok(())
}

fn is_native_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".so")
        || lower.ends_with(".dll")
        || lower.ends_with(".dylib")
        || lower.ends_with(".jnilib")
}

pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allow = false;
    for rule in rules {
        if rule_matches(rule) {
            allow = rule.action == "allow";
        }
    }
    allow
}

fn rule_matches(rule: &Rule) -> bool {
    if let Some(os_rule) = &rule.os {
        if let Some(name) = &os_rule.name {
            let want = match name.as_str() {
                "osx" => "macos",
                other => other,
            };
            if want != std::env::consts::OS {
                return false;
            }
        }
        if let Some(arch) = &os_rule.arch
            && arch != std::env::consts::ARCH
        {
            return false;
        }
    }
    if let Some(features) = &rule.features
        && !features.is_empty()
    {
        return false;
    }
    true
}

pub fn is_native(library: &Library) -> bool {
    library.natives.is_some() || library.name.contains(":natives-")
}

/// Путь Maven-артефакта: `group:artifact:version[:classifier]`.
pub fn maven_path(name: &str) -> Option<String> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let file = match parts.get(3).copied() {
        Some(classifier) if !classifier.is_empty() => {
            format!("{artifact}-{version}-{classifier}.jar")
        }
        _ => format!("{artifact}-{version}.jar"),
    };
    Some(format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file
    ))
}

/// URL, относительный путь и SHA1 библиотеки — из `downloads` или из Maven-координат.
pub fn library_file(library: &Library) -> Option<(String, String, Option<String>)> {
    if let Some(downloads) = &library.downloads
        && let Some(artifact) = &downloads.artifact
        && let Some(path) = &artifact.path
    {
        return Some((artifact.url.clone(), path.clone(), artifact.sha1.clone()));
    }
    let path = maven_path(&library.name)?;
    let base = library
        .url
        .clone()
        .unwrap_or_else(|| "https://maven.fabricmc.net/".to_string());
    let base = if base.ends_with('/') {
        base
    } else {
        format!("{base}/")
    };
    Some((format!("{base}{path}"), path, library.sha1.clone()))
}

fn native_artifact(library: &Library) -> Option<Artifact> {
    if let Some(natives) = &library.natives {
        let key = natives.get(std::env::consts::OS)?;
        let key = key.replace(
            "${arch}",
            if cfg!(target_arch = "x86_64") {
                "64"
            } else {
                "32"
            },
        );
        let classifiers = library.downloads.as_ref()?.classifiers.as_ref()?;
        return classifiers.get(&key).cloned();
    }
    library.downloads.as_ref()?.artifact.clone()
}
