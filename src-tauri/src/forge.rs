use crate::error::{Error, Result};
use crate::model::{ForgeVersion, Instance, JavaInfo};
use crate::mojang::VersionJson;
use crate::paths;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

#[derive(Debug, Deserialize)]
struct Promotions {
    #[serde(default)]
    promos: HashMap<String, String>,
}

fn loader_version(instance: &Instance) -> Result<&str> {
    instance
        .loader_version
        .as_deref()
        .filter(|version| !version.trim().is_empty())
        .ok_or_else(|| Error::Msg("У Forge-инстанса не указана версия загрузчика".to_string()))
}

fn forge_id(instance: &Instance) -> Result<String> {
    Ok(format!(
        "{}-forge-{}",
        instance.version_id,
        loader_version(instance)?
    ))
}

pub fn client_jar(instance: &Instance) -> Result<PathBuf> {
    let loader = loader_version(instance)?;
    Ok(paths::meta()
        .join("libraries")
        .join("net")
        .join("minecraftforge")
        .join("forge")
        .join(format!("{}-{}", instance.version_id, loader))
        .join(format!(
            "forge-{}-{}-client.jar",
            instance.version_id, loader
        )))
}

/// Читает установленный Forge version.json.
pub fn profile(instance: &Instance) -> Result<VersionJson> {
    let id = forge_id(instance)?;
    let path = paths::meta()
        .join("versions")
        .join(&id)
        .join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path).map_err(|_| {
        Error::Msg(format!(
            "Forge {id} не установлен. Нажми «Установить» у инстанса."
        ))
    })?;
    Ok(serde_json::from_str(&text)?)
}

/// Сливает ванильный JSON и Forge-профиль.
pub fn merge(mut vanilla: VersionJson, mut profile: VersionJson) -> VersionJson {
    let vanilla_id = vanilla.id.clone();
    profile.inherits_from = Some(vanilla_id);
    profile.assets = vanilla.assets;
    profile.asset_index = vanilla.asset_index;
    profile.downloads = vanilla.downloads;
    profile.kind = vanilla.kind;

    let mut libraries = vanilla.libraries;
    libraries.append(&mut profile.libraries);
    profile.libraries = libraries;

    let mut arguments = vanilla.arguments.take();
    if let Some(mut profile_arguments) = profile.arguments.take() {
        if let Some(existing) = arguments.as_mut() {
            existing.jvm.append(&mut profile_arguments.jvm);
            existing.game.append(&mut profile_arguments.game);
        } else {
            arguments = Some(profile_arguments);
        }
    }
    profile.arguments = arguments;
    if profile.minecraft_arguments.is_none() {
        profile.minecraft_arguments = vanilla.minecraft_arguments;
    }
    profile
}

fn version_key(value: &str) -> Vec<u64> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter_map(|part| part.parse().ok())
        .collect()
}

async fn recommended_version(client: &reqwest::Client, game: &str) -> Option<String> {
    let promotions: Promotions = client
        .get(PROMOTIONS)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    promotions
        .promos
        .get(&format!("{game}-recommended"))
        .cloned()
}

/// Список версий Forge для конкретной версии Minecraft.
pub async fn fetch_versions(client: &reqwest::Client, game: &str) -> Result<Vec<ForgeVersion>> {
    let text = client
        .get(format!("{MAVEN}/maven-metadata.xml"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let prefix = format!("{game}-");
    let mut versions = Vec::new();
    for chunk in text.split("<version>").skip(1) {
        let Some(end) = chunk.find("</version>") else {
            continue;
        };
        let full = &chunk[..end];
        if let Some(version) = full.strip_prefix(&prefix) {
            versions.push(version.to_string());
        }
    }
    versions.sort_by(|a, b| version_key(b).cmp(&version_key(a)).then_with(|| b.cmp(a)));
    versions.dedup();
    let mut recommended = recommended_version(client, game).await;
    if recommended.is_none() {
        recommended = versions.first().cloned();
    }
    Ok(versions
        .into_iter()
        .map(|version| ForgeVersion {
            recommended: recommended.as_deref() == Some(version.as_str()),
            version,
        })
        .collect())
}

fn pick_java(game: &str) -> Result<JavaInfo> {
    let preferred = crate::preferred_java_major(game);
    let javas = crate::java::detect();
    for major in [preferred, 17, 21, 25] {
        if let Some(java) = javas.iter().find(|java| java.major == major) {
            return Ok(java.clone());
        }
    }
    javas
        .into_iter()
        .next()
        .ok_or_else(|| Error::Msg("Java не найдена для установки Forge".to_string()))
}

/// Устанавливает Forge официальным инсталлером в общий `meta`.
pub async fn ensure_installed(client: &reqwest::Client, instance: &Instance) -> Result<()> {
    let game = instance.version_id.clone();
    let loader = loader_version(instance)?.to_string();
    let id = format!("{game}-forge-{loader}");
    let version_path = paths::meta()
        .join("versions")
        .join(&id)
        .join(format!("{id}.json"));
    let client_path = client_jar(instance)?;
    if version_path.is_file() && client_path.is_file() {
        return Ok(());
    }

    let installer_dir = paths::meta().join("forge").join("installers");
    std::fs::create_dir_all(&installer_dir)?;
    let installer = installer_dir.join(format!("forge-{game}-{loader}-installer.jar"));
    let url = format!("{MAVEN}/{game}-{loader}/forge-{game}-{loader}-installer.jar");
    crate::mojang::download(client, &url, &installer, None).await?;

    let meta = paths::meta();
    std::fs::create_dir_all(&meta)?;
    let profiles = meta.join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, r#"{"profiles":{},"settings":{},"version":3}"#)?;
    }

    let java = pick_java(&game)?;
    let log_path = meta.join("forge").join(format!("installer-{id}.log"));
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log = std::fs::File::create(&log_path)?;

    let mut command = Command::new(&java.path);
    crate::process::hide_console(&mut command);
    command
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(&meta);
    command.stdout(Stdio::from(log.try_clone()?));
    command.stderr(Stdio::from(log));
    if let Some(preload) = zlib_preload()? {
        command.env("LD_PRELOAD", preload);
    }

    let status = command.status().map_err(|error| {
        Error::Msg(format!(
            "Не удалось запустить Java для установки Forge: {error}"
        ))
    })?;
    if !status.success() {
        let text = std::fs::read_to_string(&log_path).unwrap_or_default();
        let hint = if text.contains("Processor failed, invalid outputs") {
            "\nПохоже на проблему zlib (CachyOS). Нужен libz 1.2.x: поставь Steam Linux Runtime или обычный zlib."
        } else {
            ""
        };
        return Err(Error::Msg(format!(
            "Установка Forge не удалась.{hint}\n{}",
            last_lines(&text, 30)
        )));
    }
    if !version_path.is_file() || !client_path.is_file() {
        return Err(Error::Msg(
            "Forge установился, но version.json или client.jar не найдены".to_string(),
        ));
    }
    Ok(())
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

#[cfg(target_os = "linux")]
fn zlib_preload() -> Result<Option<PathBuf>> {
    if !system_zlib_is_ng() {
        return Ok(None);
    }
    if let Some(path) = find_compat_zlib() {
        return Ok(Some(path));
    }
    Err(Error::Msg(
        "Для установки Forge на CachyOS нужен zlib 1.2.x (системный zlib-ng даёт неверные хеши). \
         Поставь Steam Linux Runtime или пакет с обычным zlib."
            .to_string(),
    ))
}

#[cfg(not(target_os = "linux"))]
fn zlib_preload() -> Result<Option<PathBuf>> {
    Ok(None)
}

#[cfg(target_os = "linux")]
fn system_zlib_is_ng() -> bool {
    for path in [
        "/usr/lib/libz.so.1",
        "/usr/lib64/libz.so.1",
        "/lib/x86_64-linux-gnu/libz.so.1",
    ] {
        if let Ok(target) = std::fs::read_link(path)
            && target.to_string_lossy().contains("zlib-ng")
        {
            return true;
        }
        if let Ok(canonical) = std::fs::canonicalize(path)
            && canonical.to_string_lossy().contains("zlib-ng")
        {
            return true;
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn find_compat_zlib() -> Option<PathBuf> {
    for path in [
        "/usr/lib/libz.so.1.2.11",
        "/usr/lib64/libz.so.1.2.11",
        "/lib/x86_64-linux-gnu/libz.so.1.2.11",
        "/usr/lib/x86_64-linux-gnu/libz.so.1.2.11",
    ] {
        if Path::new(path).is_file() {
            return Some(PathBuf::from(path));
        }
    }
    let data = dirs::data_dir()?;
    for base in [
        data.join("Steam/steamapps/common/SteamLinuxRuntime_sniper"),
        data.join("Steam/steamapps/common/SteamLinuxRuntime_4"),
        data.join("Steam/steamapps/common/SteamLinuxRuntime"),
        data.join("Steam/steamrt64"),
    ] {
        if let Some(path) = find_file(&base, "libz.so.1.2.11", 10) {
            return Some(path);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn is_32bit_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/i386")
        || text.contains("/i686")
        || text.contains("/x86/")
        || text.contains("i386-linux")
        || text.contains("i686-linux")
}

#[cfg(target_os = "linux")]
fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() || (file_type.is_symlink() && path.is_file()) {
            if entry.file_name().to_string_lossy() == name && !is_32bit_path(&path) {
                return Some(path);
            }
        } else if file_type.is_dir()
            && let Some(found) = find_file(&path, name, depth - 1)
        {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mojang::{Arguments, Downloads, Library};

    fn version(id: &str, main: &str) -> VersionJson {
        VersionJson {
            id: id.to_string(),
            main_class: main.to_string(),
            assets: "5".to_string(),
            asset_index: None,
            downloads: Downloads::default(),
            libraries: Vec::new(),
            arguments: Some(Arguments::default()),
            minecraft_arguments: None,
            kind: Some("release".to_string()),
            inherits_from: None,
        }
    }

    fn library(name: &str) -> Library {
        Library {
            name: name.to_string(),
            url: None,
            sha1: None,
            downloads: None,
            natives: None,
            rules: Vec::new(),
        }
    }

    #[test]
    fn merge_keeps_vanilla_and_adds_forge() {
        let mut vanilla = version("1.20.1", "net.minecraft.client.main.Main");
        vanilla.libraries.push(library("vanilla:lib:1"));
        let mut profile = version(
            "1.20.1-forge-47.4.20",
            "cpw.mods.bootstraplauncher.BootstrapLauncher",
        );
        profile.libraries.push(library("forge:lib:1"));
        profile
            .arguments
            .as_mut()
            .unwrap()
            .game
            .push(serde_json::json!("--launchTarget"));

        let merged = merge(vanilla, profile);
        assert_eq!(
            merged.main_class,
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
        assert_eq!(merged.inherits_from.as_deref(), Some("1.20.1"));
        assert_eq!(merged.assets, "5");
        assert_eq!(merged.libraries.len(), 2);
        assert_eq!(
            merged
                .arguments
                .as_ref()
                .unwrap()
                .game
                .last()
                .and_then(|value| value.as_str()),
            Some("--launchTarget")
        );
    }

    #[test]
    fn version_key_compares_numerically() {
        assert!(version_key("47.4.20") > version_key("47.4.9"));
        assert!(version_key("47.10.0") > version_key("47.9.9"));
    }
}
