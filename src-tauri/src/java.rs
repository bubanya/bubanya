use crate::error::{Error, Result};
use crate::model::JavaInfo;
use crate::paths;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn java_exe() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// Ищет Java в JAVA_HOME, PATH, системных каталогах и рантаймах PrismLauncher.
pub fn detect() -> Vec<JavaInfo> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(java_exe()));
    }
    if let Some(path) = which(java_exe()) {
        candidates.push(path);
    }

    #[cfg(target_os = "linux")]
    {
        push_jdk_children(&mut candidates, Path::new("/usr/lib/jvm"));
    }

    #[cfg(target_os = "windows")]
    {
        let mut bases: Vec<PathBuf> = Vec::new();
        for key in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(key) {
                let root = PathBuf::from(root);
                bases.push(root.join("Java"));
                bases.push(root.join("Eclipse Adoptium"));
                bases.push(root.join("Eclipse Foundation"));
                bases.push(root.join("Microsoft"));
                bases.push(root.join("Zulu"));
                bases.push(root.join("BellSoft"));
                bases.push(root.join("Amazon Corretto"));
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            bases.push(local.join("Programs").join("Eclipse Adoptium"));
            bases.push(local.join("Programs").join("Microsoft"));
        }
        for base in bases {
            push_jdk_children(&mut candidates, &base);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(entries) = std::fs::read_dir("/Library/Java/JavaVirtualMachines") {
            for entry in entries.flatten() {
                candidates.push(
                    entry
                        .path()
                        .join("Contents")
                        .join("Home")
                        .join("bin")
                        .join(java_exe()),
                );
            }
        }
    }

    for data in [dirs::data_dir(), dirs::data_local_dir()]
        .into_iter()
        .flatten()
    {
        for base in ["PrismLauncher/java", "PrismLauncher/java-runtimes"] {
            push_jdk_children(&mut candidates, &data.join(base));
        }
    }

    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let canonical = std::fs::canonicalize(&candidate).unwrap_or(candidate);
        if !seen.insert(canonical.clone()) {
            continue;
        }
        if let Some(info) = probe(&canonical) {
            found.push(info);
        }
    }
    found.extend(list_managed());
    found.sort_by_key(|a| std::cmp::Reverse(a.major));
    found
}

pub fn managed_dir() -> PathBuf {
    paths::root().join("java")
}

pub fn list_managed() -> Vec<JavaInfo> {
    let dir = managed_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let candidates = [
            root.join("bin").join(java_exe()),
            root.join("Contents")
                .join("Home")
                .join("bin")
                .join(java_exe()),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                if let Some(info) = probe(&candidate) {
                    found.push(info);
                }
                break;
            }
        }
    }
    found
}

pub fn remove_managed(major: u32) -> Result<()> {
    let dir = managed_dir().join(major.to_string());
    if dir.is_dir() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    link: String,
    name: String,
    checksum: String,
}

/// Скачивает и устанавливает JRE Adoptium в data/java/<major>.
pub async fn install_managed(client: &reqwest::Client, major: u32) -> Result<JavaInfo> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jre&os={os}&vendor=eclipse"
    );
    let assets: Vec<AdoptiumAsset> = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let package = assets
        .into_iter()
        .next()
        .ok_or_else(|| Error::Msg(format!("Нет JRE для Java {major}")))?
        .binary
        .package;

    let tmp = paths::root().join("tmp");
    std::fs::create_dir_all(&tmp)?;
    let archive = tmp.join(&package.name);
    let bytes = client
        .get(&package.link)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let checksum = hex::encode(Sha256::digest(&bytes));
    if !checksum.eq_ignore_ascii_case(&package.checksum) {
        return Err(Error::Msg("SHA256 Java-архива не совпал".to_string()));
    }
    std::fs::write(&archive, &bytes)?;

    let extract = tmp.join(format!("java-{major}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&extract)?;
    #[cfg(target_os = "windows")]
    {
        let file = std::fs::File::open(&archive)?;
        let mut zip = zip::ZipArchive::new(file)?;
        zip.extract(&extract)?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let file = std::fs::File::open(&archive)?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&extract)?;
    }

    let top = std::fs::read_dir(&extract)?
        .flatten()
        .find(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .ok_or_else(|| Error::Msg("В архиве Java нет папки".to_string()))?;
    let target = managed_dir().join(major.to_string());
    if target.exists() {
        std::fs::remove_dir_all(&target)?;
    }
    std::fs::create_dir_all(managed_dir())?;
    std::fs::rename(&top, &target)?;
    let _ = std::fs::remove_file(&archive);
    let _ = std::fs::remove_dir_all(&extract);

    let candidates = [
        target.join("bin").join(java_exe()),
        target
            .join("Contents")
            .join("Home")
            .join("bin")
            .join(java_exe()),
    ];
    let java = candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| Error::Msg("В установленной Java нет bin/java".to_string()))?;
    probe(&java).ok_or_else(|| Error::Msg("Не удалось проверить установленную Java".to_string()))
}

fn push_jdk_children(candidates: &mut Vec<PathBuf>, base: &Path) {
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let java = entry.path().join("bin").join(java_exe());
            if java.is_file() {
                candidates.push(java);
            }
        }
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn probe(path: &Path) -> Option<JavaInfo> {
    let mut command = Command::new(path);
    command.arg("-version");
    crate::process::hide_console(&mut command);
    let output = command.output().ok()?;
    let mut text = String::from_utf8_lossy(&output.stderr).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    let version = parse_version(&text)?;
    let major = parse_major(&version)?;
    Some(JavaInfo {
        path: path.to_string_lossy().to_string(),
        version,
        major,
    })
}

fn parse_version(text: &str) -> Option<String> {
    if let Some(index) = text.find("version \"") {
        let rest = &text[index + 9..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
        if trimmed.contains('.') && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn parse_major(version: &str) -> Option<u32> {
    let mut parts = version.split('.');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}
