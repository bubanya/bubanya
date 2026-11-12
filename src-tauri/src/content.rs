use crate::error::{Error, Result};
use crate::model::{ContentFile, WorldInfo};
use crate::paths;
use std::path::{Path, PathBuf};

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn pack_dir(instance_id: &str, kind: &str) -> Result<PathBuf> {
    let sub = match kind {
        "resourcepacks" => "resourcepacks",
        "shaderpacks" => "shaderpacks",
        _ => return Err(Error::Msg("Неизвестный тип контента".to_string())),
    };
    let dir = paths::instance_dir(instance_id).join("game").join(sub);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn saves_dir(instance_id: &str) -> Result<PathBuf> {
    let dir = paths::instance_dir(instance_id).join("game").join("saves");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn dir_size(path: &Path) -> u64 {
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

fn modified_millis(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn display_name(filename: &str) -> String {
    filename
        .trim_end_matches(".disabled")
        .trim_end_matches(".zip")
        .to_string()
}

fn content_entry(path: &Path, filename: &str) -> ContentFile {
    let is_dir = path.is_dir();
    let enabled = !filename.ends_with(".disabled");
    ContentFile {
        filename: filename.to_string(),
        name: display_name(filename),
        size: if is_dir {
            dir_size(path)
        } else {
            std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
        },
        enabled,
        modified: modified_millis(path),
    }
}

pub fn list_packs(instance_id: &str, kind: &str) -> Result<Vec<ContentFile>> {
    let dir = pack_dir(instance_id, kind)?;
    let mut items = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with('.') {
            continue;
        }
        let is_pack =
            path.is_dir() || filename.ends_with(".zip") || filename.ends_with(".zip.disabled");
        if !is_pack {
            continue;
        }
        items.push(content_entry(&path, &filename));
    }
    items.sort_by_key(|a| a.name.to_lowercase());
    Ok(items)
}

pub fn toggle_pack(instance_id: &str, kind: &str, filename: &str) -> Result<bool> {
    let dir = pack_dir(instance_id, kind)?;
    let path = dir.join(filename);
    if !path.exists() {
        return Err(Error::Msg("Файл не найден".to_string()));
    }
    let (target, enabled) = if filename.ends_with(".disabled") {
        (dir.join(filename.trim_end_matches(".disabled")), true)
    } else {
        (dir.join(format!("{filename}.disabled")), false)
    };
    std::fs::rename(&path, &target)?;
    Ok(enabled)
}

pub fn delete_pack(instance_id: &str, kind: &str, filename: &str) -> Result<()> {
    let dir = pack_dir(instance_id, kind)?;
    let path = dir.join(filename);
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else if path.is_file() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn import_pack(instance_id: &str, kind: &str, source: &str) -> Result<ContentFile> {
    let source_path = Path::new(source);
    let filename = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::Msg("Некорректный путь".to_string()))?
        .to_string();
    if !filename.to_lowercase().ends_with(".zip") {
        return Err(Error::Msg("Нужен .zip-файл".to_string()));
    }
    let dir = pack_dir(instance_id, kind)?;
    let destination = dir.join(&filename);
    if source_path != destination {
        std::fs::copy(source_path, &destination)?;
    }
    let disabled = dir.join(format!("{filename}.disabled"));
    if disabled.exists() {
        let _ = std::fs::remove_file(disabled);
    }
    Ok(content_entry(&destination, &filename))
}

pub fn pack_dir_for_command(instance_id: &str, kind: &str) -> Result<String> {
    Ok(pack_dir(instance_id, kind)?.to_string_lossy().to_string())
}

fn sanitize_folder(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() {
        "world".to_string()
    } else {
        cleaned
    }
}

fn unique_world_folder(saves: &Path, base: &str) -> String {
    let base = sanitize_folder(base);
    if !saves.join(&base).exists() {
        return base;
    }
    let mut counter = 2;
    loop {
        let candidate = format!("{base}-{counter}");
        if !saves.join(&candidate).exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn world_info(path: &Path, folder: &str) -> WorldInfo {
    WorldInfo {
        folder: folder.to_string(),
        name: folder.to_string(),
        size: dir_size(path),
        last_modified: modified_millis(path),
    }
}

pub fn list_worlds(instance_id: &str) -> Result<Vec<WorldInfo>> {
    let dir = saves_dir(instance_id)?;
    let mut worlds = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let folder = entry.file_name().to_string_lossy().to_string();
        if !path.is_dir() || folder.starts_with('.') {
            continue;
        }
        worlds.push(world_info(&path, &folder));
    }
    worlds.sort_by_key(|a| std::cmp::Reverse(a.last_modified));
    Ok(worlds)
}

pub fn delete_world(instance_id: &str, folder: &str) -> Result<()> {
    let dir = saves_dir(instance_id)?;
    if folder.contains('/') || folder.contains('\\') || folder == "." || folder == ".." {
        return Err(Error::Msg("Некорректное имя мира".to_string()));
    }
    let path = dir.join(folder);
    if path.is_dir() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    dir: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| Error::Msg("Не удалось собрать путь в архиве".to_string()))?;
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

pub fn backup_world(instance_id: &str, folder: &str) -> Result<String> {
    let saves = saves_dir(instance_id)?;
    let world = saves.join(folder);
    if !world.is_dir() {
        return Err(Error::Msg("Мир не найден".to_string()));
    }
    let backups = paths::root().join("backups");
    std::fs::create_dir_all(&backups)?;
    let path = backups.join(format!("{}-{}.zip", sanitize_folder(folder), now_millis()));
    let file = std::fs::File::create(&path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    add_dir_to_zip(&mut zip, &world, &world, options)?;
    zip.finish()?;
    Ok(path.to_string_lossy().to_string())
}

pub fn import_world(instance_id: &str, source: &str) -> Result<WorldInfo> {
    let source_path = Path::new(source);
    let file = std::fs::File::open(source_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut prefix: Option<String> = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let text = name.to_string_lossy().replace('\\', "/");
        if text == "level.dat" {
            prefix = Some(String::new());
            break;
        }
        if let Some(folder) = text.strip_suffix("level.dat")
            && folder.ends_with('/')
        {
            prefix = Some(folder.to_string());
            break;
        }
    }
    let prefix =
        prefix.ok_or_else(|| Error::Msg("В архиве нет level.dat — это не мир".to_string()))?;
    let base_name = if prefix.is_empty() {
        source_path
            .file_stem()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| "world".to_string())
    } else {
        prefix
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("world")
            .to_string()
    };
    let saves = saves_dir(instance_id)?;
    let folder = unique_world_folder(&saves, &base_name);
    let target = saves.join(&folder);
    std::fs::create_dir_all(&target)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let text = name.to_string_lossy().replace('\\', "/");
        let Some(rest) = text.strip_prefix(&prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let Some(relative) = crate::modrinth::safe_relative(rest) else {
            continue;
        };
        let out = target.join(relative);
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
    Ok(world_info(&target, &folder))
}

pub fn worlds_dir_for_command(instance_id: &str) -> Result<String> {
    Ok(saves_dir(instance_id)?.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_are_clean() {
        assert_eq!(
            display_name("FreshAnimations_v1.9.zip"),
            "FreshAnimations_v1.9"
        );
        assert_eq!(display_name("pack.zip.disabled"), "pack");
    }

    #[test]
    fn folder_names_are_sanitized() {
        assert_eq!(sanitize_folder("my/world"), "my_world");
        assert_eq!(sanitize_folder(".."), "world");
        assert_eq!(sanitize_folder("  "), "world");
    }

    #[test]
    fn unique_world_folder_appends_counter() {
        let base = std::env::temp_dir().join(format!("bubanya-worlds-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("world")).unwrap();
        std::fs::create_dir_all(base.join("world-2")).unwrap();
        assert_eq!(unique_world_folder(&base, "world"), "world-3");
        assert_eq!(unique_world_folder(&base, "new"), "new");
        std::fs::remove_dir_all(base).ok();
    }
}
