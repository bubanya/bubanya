use crate::download::DownloadControl;
use crate::error::{Error, Result};
use crate::model::{InstalledMod, Instance, ModrinthHit, ModrinthSearchResult, ModrinthVersion};
use crate::paths;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;

const API: &str = "https://api.modrinth.com/v2";
const USER_AGENT: &str = concat!("Bubanya/", env!("CARGO_PKG_VERSION"), " (dev.asswa.bubanya)");

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
    total_hits: u64,
    offset: u64,
    limit: u64,
}

#[derive(Debug, Deserialize)]
struct Hit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    #[serde(default)]
    categories: Vec<String>,
    project_type: String,
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    versions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectInfo {
    #[serde(default)]
    icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Version {
    id: String,
    project_id: String,
    name: String,
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    files: Vec<VersionFile>,
    #[serde(rename = "date_published", default)]
    date_published: String,
    #[serde(default)]
    dependencies: Vec<VersionDependency>,
}

#[derive(Debug, Deserialize)]
struct VersionDependency {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    dependency_type: String,
}

#[derive(Debug, Deserialize)]
struct VersionFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    hashes: Hashes,
}

#[derive(Debug, Deserialize, Default)]
struct Hashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct ModIndex {
    #[serde(default)]
    pub(crate) entries: HashMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IndexEntry {
    pub(crate) project_id: String,
    pub(crate) title: String,
    pub(crate) version: String,
}

pub async fn search(
    client: &reqwest::Client,
    query: &str,
    game_version: &str,
    loader: &str,
    project_type: &str,
    sort: &str,
    offset: u64,
    limit: u64,
) -> Result<ModrinthSearchResult> {
    let mut facets: Vec<Vec<String>> = Vec::new();
    facets.push(vec![format!("project_type:{project_type}")]);
    if !game_version.is_empty() {
        facets.push(vec![format!("versions:{game_version}")]);
    }
    if !loader.is_empty() {
        facets.push(vec![format!("categories:{loader}")]);
    }
    let facets_json = serde_json::to_string(&facets)?;
    let mut url = reqwest::Url::parse(&format!("{API}/search"))
        .map_err(|error| Error::Msg(error.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("query", query);
        pairs.append_pair("facets", &facets_json);
        if !sort.is_empty() {
            pairs.append_pair("index", sort);
        }
        pairs.append_pair("limit", &limit.to_string());
        pairs.append_pair("offset", &offset.to_string());
    }
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?;
    let data: SearchResponse = response.json().await?;
    Ok(ModrinthSearchResult {
        hits: data
            .hits
            .into_iter()
            .map(|hit| ModrinthHit {
                project_id: hit.project_id,
                slug: hit.slug,
                title: hit.title,
                description: hit.description,
                categories: hit.categories,
                project_type: hit.project_type,
                downloads: hit.downloads,
                icon_url: hit.icon_url,
                author: hit.author,
                versions: hit.versions,
            })
            .collect(),
        total: data.total_hits,
        offset: data.offset,
        limit: data.limit,
    })
}

pub async fn fetch_versions(
    client: &reqwest::Client,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<ModrinthVersion>> {
    let versions = fetch_raw_versions(client, project_id, game_version, loader).await?;
    Ok(versions
        .into_iter()
        .map(|version| {
            let file = version
                .files
                .iter()
                .find(|file| file.primary)
                .or_else(|| version.files.first());
            ModrinthVersion {
                id: version.id,
                name: version.name,
                version_number: version.version_number,
                game_versions: version.game_versions,
                loaders: version.loaders,
                filename: file.map(|file| file.filename.clone()).unwrap_or_default(),
                size: file.map(|file| file.size).unwrap_or(0),
                date_published: version.date_published,
            }
        })
        .collect())
}

async fn fetch_raw_versions(
    client: &reqwest::Client,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<Version>> {
    let mut url = reqwest::Url::parse(&format!("{API}/project/{project_id}/version"))
        .map_err(|error| Error::Msg(error.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        if !game_version.is_empty() {
            pairs.append_pair("game_versions", &format!("[\"{game_version}\"]"));
        }
        if !loader.is_empty() {
            pairs.append_pair("loaders", &format!("[\"{loader}\"]"));
        }
    }
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

/// Самая свежая версия проекта: (version_id, version_number, name).
/// Для проверки обновлений модпаков.
pub(crate) async fn fetch_latest_version(
    client: &reqwest::Client,
    project_id: &str,
) -> Result<Option<(String, String, String)>> {
    let versions = fetch_raw_versions(client, project_id, "", "").await?;
    Ok(versions
        .into_iter()
        .next()
        .map(|version| (version.id, version.version_number, version.name)))
}

async fn fetch_version_by_id(client: &reqwest::Client, version_id: &str) -> Result<Version> {
    let response = client
        .get(format!("{API}/version/{version_id}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

pub async fn install_version(
    client: &reqwest::Client,
    instance_id: &str,
    version_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<InstalledMod> {
    let version = fetch_version_by_id(client, version_id).await?;
    install_version_tree(client, instance_id, game_version, loader, version).await
}

pub async fn install_mod(
    client: &reqwest::Client,
    instance_id: &str,
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<InstalledMod> {
    let versions = fetch_raw_versions(client, project_id, game_version, loader).await?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| Error::Msg("Нет версии, совместимой с инстансом".to_string()))?;
    install_version_tree(client, instance_id, game_version, loader, version).await
}

async fn install_version_tree(
    client: &reqwest::Client,
    instance_id: &str,
    game_version: &str,
    loader: &str,
    main: Version,
) -> Result<InstalledMod> {
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    let mut index = read_index(&mods_dir);

    let main_project = main.project_id.clone();
    let main_installed = install_version_file(client, &mods_dir, &main, &mut index).await?;

    let mut visited = HashSet::new();
    visited.insert(main_project);
    let mut queue: VecDeque<(String, Option<String>)> = VecDeque::new();
    push_required_dependencies(&main, &mut queue, &mut visited, &index);

    while let Some((project_id, version_id)) = queue.pop_front() {
        if visited.contains(&project_id) {
            continue;
        }
        if index
            .entries
            .values()
            .any(|entry| entry.project_id == project_id)
        {
            visited.insert(project_id);
            continue;
        }
        let version = match version_id {
            Some(id) => fetch_version_by_id(client, &id).await?,
            None => {
                let versions =
                    fetch_raw_versions(client, &project_id, game_version, loader).await?;
                versions.into_iter().next().ok_or_else(|| {
                    Error::Msg(format!(
                        "Не найдена совместимая версия зависимости {project_id}"
                    ))
                })?
            }
        };
        let actual_project = version.project_id.clone();
        if !visited.insert(actual_project) {
            continue;
        }
        install_version_file(client, &mods_dir, &version, &mut index).await?;
        push_required_dependencies(&version, &mut queue, &mut visited, &index);
    }

    Ok(main_installed)
}

fn push_required_dependencies(
    version: &Version,
    queue: &mut VecDeque<(String, Option<String>)>,
    visited: &mut HashSet<String>,
    index: &ModIndex,
) {
    for dependency in &version.dependencies {
        if dependency.dependency_type != "required" {
            continue;
        }
        let Some(project_id) = &dependency.project_id else {
            continue;
        };
        if visited.contains(project_id) {
            continue;
        }
        if index
            .entries
            .values()
            .any(|entry| entry.project_id == *project_id)
        {
            visited.insert(project_id.clone());
            continue;
        }
        queue.push_back((project_id.clone(), dependency.version_id.clone()));
    }
}

async fn install_version_file(
    client: &reqwest::Client,
    mods_dir: &Path,
    version: &Version,
    index: &mut ModIndex,
) -> Result<InstalledMod> {
    let project_id = version.project_id.clone();
    let version_name = version.name.clone();
    let version_number = version.version_number.clone();
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| Error::Msg("У версии нет файлов".to_string()))?;
    let old_bases: Vec<String> = index
        .entries
        .iter()
        .filter(|(_, entry)| entry.project_id == project_id)
        .map(|(base, _)| base.clone())
        .collect();
    let was_disabled = old_bases
        .iter()
        .any(|base| mods_dir.join(format!("{base}.disabled")).exists());
    let destination = mods_dir.join(&file.filename);
    crate::mojang::download(client, &file.url, &destination, file.hashes.sha1.as_deref()).await?;
    for base in &old_bases {
        let old_enabled = mods_dir.join(base);
        let old_disabled = mods_dir.join(format!("{base}.disabled"));
        if old_enabled != destination && old_enabled.exists() {
            let _ = std::fs::remove_file(&old_enabled);
        }
        if old_disabled != destination && old_disabled.exists() {
            let _ = std::fs::remove_file(&old_disabled);
        }
        index.entries.remove(base);
    }
    let new_base = base_name(&file.filename).to_string();
    let mut final_filename = file.filename.clone();
    if was_disabled {
        let target = mods_dir.join(format!("{new_base}.disabled"));
        if destination != target {
            let _ = std::fs::remove_file(&target);
            std::fs::rename(&destination, &target)?;
        }
        final_filename = format!("{new_base}.disabled");
    }
    index.entries.insert(
        new_base,
        IndexEntry {
            project_id: project_id.clone(),
            title: version_name.clone(),
            version: version_number.clone(),
        },
    );
    write_index(mods_dir, index)?;
    Ok(InstalledMod {
        filename: final_filename,
        name: version_name,
        version: version_number,
        size: file.size,
        enabled: !was_disabled,
        project_id: Some(project_id),
    })
}

fn read_jar_text(path: &Path, name: &str) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).ok()?;
    Some(text)
}

fn toml_string(line: &str) -> Option<String> {
    let (_, value) = line.split_once('=')?;
    let value = value
        .trim()
        .trim_matches(|character| character == '"' || character == '\'');
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn read_jar_metadata(path: &Path) -> Option<(String, String)> {
    if let Some(text) = read_jar_text(path, "fabric.mod.json")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let name = value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .or_else(|| value.get("id").and_then(serde_json::Value::as_str));
        if let Some(name) = name {
            let version = value
                .get("version")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            return Some((name.to_string(), version));
        }
    }
    if let Some(text) = read_jar_text(path, "quilt.mod.json")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let loader = value.get("quilt_loader");
        let name = loader
            .and_then(|value| value.pointer("/metadata/name"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                loader
                    .and_then(|value| value.get("id"))
                    .and_then(serde_json::Value::as_str)
            });
        if let Some(name) = name {
            let version = loader
                .and_then(|value| value.get("version"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            return Some((name.to_string(), version));
        }
    }
    if let Some(text) = read_jar_text(path, "META-INF/mods.toml") {
        let mut name = None;
        let mut version = None;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("displayName") {
                name = toml_string(line);
            } else if line.starts_with("version") && version.is_none() {
                version = toml_string(line);
            }
        }
        if let Some(name) = name {
            return Some((name, version.unwrap_or_default()));
        }
    }
    None
}

/// Устанавливает вручную выбранный .jar в папку модов инстанса.
pub fn install_mod_file(instance_id: &str, source: &str) -> Result<InstalledMod> {
    let source_path = Path::new(source);
    let filename = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::Msg("Некорректный путь к моду".to_string()))?
        .to_string();
    if !filename.to_lowercase().ends_with(".jar") {
        return Err(Error::Msg("Нужен .jar-файл".to_string()));
    }
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    let destination = mods_dir.join(&filename);
    if source_path != destination {
        std::fs::copy(source_path, &destination)?;
    }
    let fallback = filename.trim_end_matches(".jar").to_string();
    let (title, version) = read_jar_metadata(&destination).unwrap_or((fallback, String::new()));
    let mut index = read_index(&mods_dir);
    index.entries.insert(
        base_name(&filename).to_string(),
        IndexEntry {
            project_id: String::new(),
            title: title.clone(),
            version: version.clone(),
        },
    );
    write_index(&mods_dir, &index)?;
    let size = std::fs::metadata(&destination)?.len();
    Ok(InstalledMod {
        filename,
        name: title,
        version,
        size,
        enabled: true,
        project_id: None,
    })
}

fn index_path(mods_dir: &Path) -> PathBuf {
    mods_dir.join(".onelite-index.json")
}

pub(crate) fn read_index(mods_dir: &Path) -> ModIndex {
    std::fs::read_to_string(index_path(mods_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub(crate) fn write_index(mods_dir: &Path, index: &ModIndex) -> Result<()> {
    std::fs::write(index_path(mods_dir), serde_json::to_string_pretty(index)?)?;
    Ok(())
}

pub(crate) fn base_name(filename: &str) -> &str {
    filename.strip_suffix(".disabled").unwrap_or(filename)
}

pub fn list_mods(instance_id: &str) -> Result<Vec<InstalledMod>> {
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    let index = read_index(&mods_dir);
    let mut mods = Vec::new();
    for entry in std::fs::read_dir(&mods_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        if !filename.ends_with(".jar") && !filename.ends_with(".jar.disabled") {
            continue;
        }
        let enabled = filename.ends_with(".jar");
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        let indexed = index.entries.get(base_name(&filename));
        mods.push(InstalledMod {
            filename: filename.clone(),
            name: indexed
                .map(|data| data.title.clone())
                .unwrap_or_else(|| filename.clone()),
            version: indexed.map(|data| data.version.clone()).unwrap_or_default(),
            size,
            enabled,
            project_id: indexed
                .map(|data| data.project_id.clone())
                .filter(|project_id| !project_id.is_empty()),
        });
    }
    mods.sort_by_key(|a| a.name.to_lowercase());
    Ok(mods)
}

pub fn toggle_mod(instance_id: &str, filename: &str) -> Result<bool> {
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    let path = mods_dir.join(filename);
    if !path.is_file() {
        return Err(Error::Msg("Мод не найден".to_string()));
    }
    let (target, enabled) = if filename.ends_with(".jar.disabled") {
        (mods_dir.join(filename.trim_end_matches(".disabled")), true)
    } else if filename.ends_with(".jar") {
        (mods_dir.join(format!("{filename}.disabled")), false)
    } else {
        return Err(Error::Msg("Это не jar-файл".to_string()));
    };
    std::fs::rename(&path, &target)?;
    Ok(enabled)
}

pub fn delete_mod(instance_id: &str, filename: &str) -> Result<()> {
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    let path = mods_dir.join(filename);
    if path.is_file() {
        std::fs::remove_file(path)?;
    }
    let mut index = read_index(&mods_dir);
    index.entries.remove(base_name(filename));
    write_index(&mods_dir, &index)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct MrpackIndex {
    #[serde(default)]
    name: String,
    #[serde(default)]
    files: Vec<MrpackFile>,
    #[serde(default)]
    dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct MrpackFile {
    path: String,
    #[serde(default)]
    hashes: Hashes,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    env: Option<MrpackEnv>,
}

#[derive(Debug, Deserialize)]
struct MrpackEnv {
    #[serde(default)]
    client: Option<String>,
}

async fn fetch_project_icon(client: &reqwest::Client, project_id: &str) -> Option<String> {
    let response = client
        .get(format!("{API}/project/{project_id}"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let project: ProjectInfo = response.json().await.ok()?;
    project.icon_url
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub files: Vec<String>,
}

fn pack_manifest_path(instance_id: &str) -> PathBuf {
    paths::instance_dir(instance_id).join("pack.json")
}

pub fn read_pack_manifest(instance_id: &str) -> Option<PackManifest> {
    let text = std::fs::read_to_string(pack_manifest_path(instance_id)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_pack_manifest(instance_id: &str, manifest: &PackManifest) -> Result<()> {
    let path = pack_manifest_path(instance_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

fn read_mrpack_index(mrpack: &Path) -> Result<MrpackIndex> {
    let file = std::fs::File::open(mrpack)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut index_file = archive.by_name("modrinth.index.json")?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut index_file, &mut text)?;
    Ok(serde_json::from_str(&text)?)
}

fn resolve_loader(index: &MrpackIndex) -> Result<(String, String, Option<String>)> {
    let minecraft = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| Error::Msg("В модпаке нет версии Minecraft".to_string()))?;
    if index.dependencies.contains_key("neoforge") {
        return Err(Error::Msg(
            "NeoForge-модпаки пока не поддерживаются".to_string(),
        ));
    }
    let fabric = index.dependencies.get("fabric-loader").cloned();
    let forge = index.dependencies.get("forge").cloned();
    let (loader, loader_version) = if let Some(fabric) = fabric {
        ("fabric".to_string(), Some(fabric))
    } else if let Some(forge) = forge {
        let forge = forge
            .strip_prefix(&format!("{minecraft}-"))
            .unwrap_or(&forge)
            .to_string();
        ("forge".to_string(), Some(forge))
    } else {
        ("vanilla".to_string(), None)
    };
    Ok((minecraft, loader, loader_version))
}

pub async fn install_modpack(
    app: &AppHandle,
    client: &reqwest::Client,
    project_id: &str,
    version_id: Option<&str>,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<Instance> {
    let version = match version_id {
        Some(id) => fetch_version_by_id(client, id).await?,
        None => {
            let versions = fetch_raw_versions(client, project_id, "", "").await?;
            versions
                .into_iter()
                .next()
                .ok_or_else(|| Error::Msg("У модпака нет версий".to_string()))?
        }
    };
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| Error::Msg("У версии модпака нет файлов".to_string()))?;
    let tmp_dir = paths::root().join("tmp");
    std::fs::create_dir_all(&tmp_dir)?;
    let mrpack = tmp_dir.join(&file.filename);
    crate::mojang::download(client, &file.url, &mrpack, file.hashes.sha1.as_deref()).await?;

    let icon = fetch_project_icon(client, project_id).await;
    let instance = install_mrpack_file(
        app,
        client,
        &mrpack,
        &version.name,
        icon,
        project_id,
        &version.id,
        &version.version_number,
        concurrency,
        cancel,
    )
    .await?;
    let _ = std::fs::remove_file(&mrpack);
    Ok(instance)
}

/// Устанавливает .mrpack из локального файла. Файл не удаляется.
pub async fn install_mrpack_file(
    app: &AppHandle,
    client: &reqwest::Client,
    mrpack: &Path,
    fallback_name: &str,
    icon: Option<String>,
    project_id: &str,
    version_id: &str,
    version_name: &str,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<Instance> {
    let index = read_mrpack_index(mrpack)?;
    let (minecraft, loader, loader_version) = resolve_loader(&index)?;
    let mut instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: if index.name.trim().is_empty() {
            fallback_name.to_string()
        } else {
            index.name.clone()
        },
        version_id: minecraft,
        loader,
        loader_version,
        created_at: now_millis(),
        last_played: None,
        installed: false,
        icon,
        ram_mb: None,
        java_path: None,
        jvm_args: None,
    };
    paths::ensure_instance(&instance)?;
    let result = install_mrpack_into(
        app,
        client,
        &mut instance,
        mrpack,
        &index,
        project_id,
        version_id,
        version_name,
        concurrency,
        cancel,
    )
    .await;
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(paths::instance_dir(&instance.id));
        return Err(error);
    }
    Ok(instance)
}

/// Обновляет существующий инстанс из .mrpack, сохраняя миры и настройки.
async fn install_mrpack_into(
    app: &AppHandle,
    client: &reqwest::Client,
    instance: &mut Instance,
    mrpack: &Path,
    index: &MrpackIndex,
    project_id: &str,
    version_id: &str,
    version_name: &str,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<()> {
    let (minecraft, loader, loader_version) = resolve_loader(index)?;
    let game_dir = paths::instance_dir(&instance.id).join("game");
    std::fs::create_dir_all(&game_dir)?;

    let mut new_files = Vec::new();
    for pack_file in &index.files {
        if pack_file.env.as_ref().and_then(|env| env.client.as_deref()) == Some("unsupported") {
            continue;
        }
        let Some(relative) = safe_relative(&pack_file.path) else {
            continue;
        };
        new_files.push(relative.to_string_lossy().replace('\\', "/"));
    }

    instance.version_id = minecraft;
    instance.loader = loader;
    instance.loader_version = loader_version;

    let mut tasks = Vec::new();
    for pack_file in &index.files {
        if pack_file.env.as_ref().and_then(|env| env.client.as_deref()) == Some("unsupported") {
            continue;
        }
        let Some(url) = pack_file.downloads.first() else {
            continue;
        };
        let Some(relative) = safe_relative(&pack_file.path) else {
            continue;
        };
        tasks.push((
            url.clone(),
            game_dir.join(relative),
            pack_file.hashes.sha1.clone(),
        ));
    }
    crate::mojang::download_many(
        app,
        client,
        &instance.id,
        "modpack",
        tasks,
        concurrency,
        cancel.clone(),
    )
    .await?;

    if let Some(old) = read_pack_manifest(&instance.id) {
        for relative in &old.files {
            if new_files.contains(relative) {
                continue;
            }
            if let Some(relative) = safe_relative(relative) {
                let path = game_dir.join(relative);
                let _ = std::fs::remove_file(path);
            }
        }
    }

    let mut mod_index = ModIndex::default();
    for pack_file in &index.files {
        let Some(relative) = safe_relative(&pack_file.path) else {
            continue;
        };
        if !relative.starts_with("mods") {
            continue;
        }
        let Some(url) = pack_file.downloads.first() else {
            continue;
        };
        let Some(project_id) = project_id_from_url(url) else {
            continue;
        };
        let Some(file_name) = relative.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        mod_index.entries.insert(
            base_name(file_name).to_string(),
            IndexEntry {
                project_id,
                title: file_name.trim_end_matches(".jar").to_string(),
                version: String::new(),
            },
        );
    }
    if !mod_index.entries.is_empty() {
        let mods_dir = game_dir.join("mods");
        std::fs::create_dir_all(&mods_dir)?;
        write_index(&mods_dir, &mod_index)?;
    }

    let archive_file = std::fs::File::open(mrpack)?;
    let mut archive = zip::ZipArchive::new(archive_file)?;
    extract_overrides(&mut archive, &game_dir)?;
    crate::mojang::install(app, client, instance, concurrency, cancel).await?;

    write_pack_manifest(
        &instance.id,
        &PackManifest {
            project_id: project_id.to_string(),
            version_id: version_id.to_string(),
            name: version_name.to_string(),
            files: new_files,
        },
    )?;
    Ok(())
}

/// Обновляет модпак-инстанс до указанной или последней версии.
pub async fn update_modpack(
    app: &AppHandle,
    client: &reqwest::Client,
    instance: &mut Instance,
    version_id: Option<&str>,
    concurrency: u32,
    cancel: Option<Arc<DownloadControl>>,
) -> Result<()> {
    let manifest = read_pack_manifest(&instance.id)
        .ok_or_else(|| Error::Msg("Этот инстанс установлен не из модпака".to_string()))?;
    if manifest.project_id.is_empty() {
        return Err(Error::Msg(
            "Инстанс установлен из файла .mrpack — обновление недоступно".to_string(),
        ));
    }
    let version = match version_id {
        Some(id) => fetch_version_by_id(client, id).await?,
        None => {
            let versions = fetch_raw_versions(client, &manifest.project_id, "", "").await?;
            versions
                .into_iter()
                .next()
                .ok_or_else(|| Error::Msg("У модпака нет версий".to_string()))?
        }
    };
    if version.project_id != manifest.project_id {
        return Err(Error::Msg("Версия принадлежит другому модпаку".to_string()));
    }
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| Error::Msg("У версии модпака нет файлов".to_string()))?;
    let tmp_dir = paths::root().join("tmp");
    std::fs::create_dir_all(&tmp_dir)?;
    let mrpack = tmp_dir.join(&file.filename);
    crate::mojang::download(client, &file.url, &mrpack, file.hashes.sha1.as_deref()).await?;
    let index = read_mrpack_index(&mrpack)?;
    let result = install_mrpack_into(
        app,
        client,
        instance,
        &mrpack,
        &index,
        &manifest.project_id,
        &version.id,
        &version.version_number,
        concurrency,
        cancel,
    )
    .await;
    let _ = std::fs::remove_file(&mrpack);
    result
}

fn extract_overrides(archive: &mut zip::ZipArchive<std::fs::File>, game_dir: &Path) -> Result<()> {
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let name_text = name.to_string_lossy().to_string();
        let rest = if let Some(rest) = name_text.strip_prefix("overrides/") {
            rest
        } else if let Some(rest) = name_text.strip_prefix("client-overrides/") {
            rest
        } else {
            continue;
        };
        let Some(relative) = safe_relative(rest) else {
            continue;
        };
        let out = game_dir.join(relative);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes)?;
        std::fs::write(&out, &bytes)?;
    }
    Ok(())
}

pub(crate) fn safe_relative(path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return None;
    }
    let mut result = PathBuf::new();
    for component in candidate.components() {
        match component {
            std::path::Component::Normal(part) => result.push(part),
            std::path::Component::CurDir => {}
            _ => return None,
        }
    }
    if result.as_os_str().is_empty() {
        None
    } else {
        Some(result)
    }
}

fn project_id_from_url(url: &str) -> Option<String> {
    let marker = "/data/";
    let index = url.find(marker)? + marker.len();
    let rest = &url[index..];
    let end = rest.find('/')?;
    Some(rest[..end].to_string())
}

pub async fn update_all(
    client: &reqwest::Client,
    instance_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<InstalledMod>> {
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    let index = read_index(&mods_dir);
    let entries: Vec<IndexEntry> = index.entries.values().cloned().collect();
    let mut updated = Vec::new();
    for entry in entries {
        if entry.project_id.trim().is_empty() {
            continue;
        }
        let versions = fetch_raw_versions(client, &entry.project_id, game_version, loader).await?;
        let Some(version) = versions.into_iter().next() else {
            continue;
        };
        if version.version_number == entry.version {
            continue;
        }
        updated
            .push(install_version_tree(client, instance_id, game_version, loader, version).await?);
    }
    Ok(updated)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Serialize)]
struct HashRequest<'a> {
    hashes: &'a [String],
    algorithm: &'a str,
}

#[derive(Debug, Deserialize)]
struct VersionFileEntry {
    project_id: String,
    name: String,
    version_number: String,
}

/// Сопоставляет jar-файлы инстанса с проектами Modrinth по SHA1 и пишет индекс.
pub async fn reindex_mods(
    client: &reqwest::Client,
    instance_id: &str,
) -> Result<Vec<InstalledMod>> {
    let mods_dir = paths::instance_dir(instance_id).join("game").join("mods");
    std::fs::create_dir_all(&mods_dir)?;
    let mut hashes = Vec::new();
    let mut by_hash: HashMap<String, String> = HashMap::new();
    for entry in std::fs::read_dir(&mods_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        if !filename.ends_with(".jar") && !filename.ends_with(".jar.disabled") {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let sha1 = hex::encode(Sha1::digest(&bytes));
        hashes.push(sha1.clone());
        by_hash.insert(sha1, base_name(&filename).to_string());
    }
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let response = client
        .post(format!("{API}/version_files"))
        .header("User-Agent", USER_AGENT)
        .json(&HashRequest {
            hashes: &hashes,
            algorithm: "sha1",
        })
        .send()
        .await?
        .error_for_status()?;
    let data: HashMap<String, VersionFileEntry> = response.json().await?;
    let mut index = read_index(&mods_dir);
    for (hash, version) in data {
        let Some(base) = by_hash.get(&hash) else {
            continue;
        };
        index.entries.insert(
            base.clone(),
            IndexEntry {
                project_id: version.project_id,
                title: version.name,
                version: version.version_number,
            },
        );
    }
    write_index(&mods_dir, &index)?;
    list_mods(instance_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_jar(entries: &[(&str, &str)]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bubanya-mod-{}.jar", uuid::Uuid::new_v4()));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, contents) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[tokio::test]
    #[ignore]
    async fn installs_required_dependencies() {
        let _guard = crate::paths::DATA_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("bubanya-dep-test-{}", uuid::Uuid::new_v4()));
        let mods = dir
            .join("instances")
            .join("dep-test")
            .join("game")
            .join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        unsafe { std::env::set_var("BUBANYA_DATA_DIR", &dir) };
        let client = reqwest::Client::builder()
            .user_agent("Bubanya/0.1")
            .http1_only()
            .build()
            .unwrap();
        let installed = install_mod(&client, "dep-test", "jei", "1.20.1", "forge")
            .await
            .unwrap();
        let index = read_index(&mods);
        assert!(installed.filename.to_lowercase().contains("jei"));
        assert!(
            index
                .entries
                .values()
                .any(|entry| entry.project_id == "7tEfOcA7")
        );
        assert!(index.entries.len() >= 2);
        unsafe { std::env::remove_var("BUBANYA_DATA_DIR") };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_fabric_metadata() {
        let path = temp_jar(&[(
            "fabric.mod.json",
            r#"{"id":"example","name":"Example Mod","version":"1.2.3"}"#,
        )]);
        let metadata = read_jar_metadata(&path).unwrap();
        assert_eq!(metadata.0, "Example Mod");
        assert_eq!(metadata.1, "1.2.3");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn reads_forge_metadata() {
        let path = temp_jar(&[(
            "META-INF/mods.toml",
            "modLoader=\"javafml\"\n[[mods]]\nmodId=\"example\"\ndisplayName=\"Forge Example\"\nversion=\"4.5.6\"\n",
        )]);
        let metadata = read_jar_metadata(&path).unwrap();
        assert_eq!(metadata.0, "Forge Example");
        assert_eq!(metadata.1, "4.5.6");
        std::fs::remove_file(path).ok();
    }
}
