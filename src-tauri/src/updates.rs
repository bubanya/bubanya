//! Фоновые проверки обновлений: релиза лаунчера на GitHub и модпаков на Modrinth.
//! Результаты кэшируются в каталоге данных на 24 часа, чтобы не долбить API
//! при каждом запуске и каждой смене экрана.

use crate::error::Result;
use crate::model::Instance;
use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const GITHUB_RELEASES_URL: &str = "https://api.github.com/repos/bubanya/bubanya/releases/latest";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestRelease {
    pub tag: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdate {
    pub available: bool,
    pub current: String,
    pub latest: Option<LatestRelease>,
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdateInfo {
    pub instance_id: String,
    pub project_id: String,
    pub current_version_id: String,
    pub latest_version_id: String,
    pub latest_version_number: String,
    pub latest_name: String,
    pub has_update: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheckCache {
    #[serde(default)]
    checked_at: i64,
    #[serde(default)]
    dismissed_tag: Option<String>,
    #[serde(default)]
    latest_tag: Option<String>,
    #[serde(default)]
    latest_name: Option<String>,
    #[serde(default)]
    latest_url: Option<String>,
}

impl UpdateCheckCache {
    fn latest(&self) -> Option<LatestRelease> {
        Some(LatestRelease {
            tag: self.latest_tag.clone()?,
            name: self.latest_name.clone().unwrap_or_default(),
            url: self.latest_url.clone()?,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackUpdatesCache {
    #[serde(default)]
    projects: HashMap<String, CachedPackLatest>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedPackLatest {
    checked_at: i64,
    version_id: String,
    version_number: String,
    name: String,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn read_cache<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn write_cache<T: serde::Serialize>(path: &std::path::Path, value: &T) {
    if let Ok(text) = serde_json::to_string_pretty(value)
        && std::fs::write(path, text).is_ok()
    {
        return;
    }
    let _ = paths::ensure();
    let _ = std::fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default());
}

/// Сравнивает semver-подобные строки («v0.2.0» > «0.1.0»).
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> {
        text.trim_start_matches('v')
            .split('.')
            .map(|part| part.split(['-', '+']).next().unwrap_or("0"))
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let left = parse(a);
    let right = parse(b);
    for index in 0..left.len().max(right.len()) {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        if l != r {
            return l > r;
        }
    }
    false
}

async fn fetch_latest_release(client: &reqwest::Client) -> crate::error::Result<LatestRelease> {
    let value: serde_json::Value = client
        .get(GITHUB_RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let tag = value
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| crate::error::Error::Msg("GitHub не вернул tag_name".to_string()))?;
    let url = value
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| crate::error::Error::Msg("GitHub не вернул html_url".to_string()))?;
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(tag)
        .to_string();
    Ok(LatestRelease {
        tag: tag.to_string(),
        name,
        url: url.to_string(),
    })
}

/// Проверяет свежий релиз лаунчера. Сеть трогаем не чаще раза в сутки,
/// остальное берём из кэша, поэтому команда дешёвая и не падает без сети.
pub async fn check_launcher_update(client: &reqwest::Client) -> LauncherUpdate {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let cache_path = paths::root().join("update-check.json");
    let mut cache: UpdateCheckCache = read_cache(&cache_path).unwrap_or_default();

    let fresh = now_millis().saturating_sub(cache.checked_at) < DAY_MS;
    if !fresh {
        match fetch_latest_release(client).await {
            Ok(latest) => {
                cache.checked_at = now_millis();
                cache.latest_tag = Some(latest.tag.clone());
                cache.latest_name = Some(latest.name.clone());
                cache.latest_url = Some(latest.url.clone());
                write_cache(&cache_path, &cache);
            }
            // Сеть недоступна — молча используем старый кэш, если он есть.
            Err(_) => {}
        }
    }

    let latest = cache.latest();
    let available = latest
        .as_ref()
        .is_some_and(|latest| version_gt(&latest.tag, &current));
    let dismissed = latest
        .as_ref()
        .is_some_and(|latest| cache.dismissed_tag.as_deref() == Some(latest.tag.as_str()));
    LauncherUpdate {
        available,
        current,
        latest,
        dismissed,
    }
}

/// Запоминает, что пользователь скрыл баннер текущего релиза.
pub fn dismiss_launcher_update() -> Result<()> {
    let cache_path = paths::root().join("update-check.json");
    let mut cache: UpdateCheckCache = read_cache(&cache_path).unwrap_or_default();
    cache.dismissed_tag = cache.latest_tag.clone();
    write_cache(&cache_path, &cache);
    Ok(())
}

/// Для каждого инстанса, установленного из Modrinth-модпака, выясняет,
/// есть ли версия нове установленной. Ошибки по отдельным проектам
/// пропускаем: один недоступный модпак не должен ломать проверку целиком.
pub async fn check_pack_updates(
    client: &reqwest::Client,
    instances: &[Instance],
) -> Vec<PackUpdateInfo> {
    let cache_path = paths::root().join("pack-updates.json");
    let mut cache: PackUpdatesCache = read_cache(&cache_path).unwrap_or_default();
    let mut dirty = false;
    let mut result = Vec::new();

    for instance in instances {
        let Some(manifest) = crate::modrinth::read_pack_manifest(&instance.id) else {
            continue;
        };
        if manifest.project_id.is_empty() {
            continue;
        }
        let project_id = manifest.project_id.clone();
        let entry = match cache.projects.get(&project_id) {
            Some(entry) if now_millis().saturating_sub(entry.checked_at) < DAY_MS => {
                entry.clone()
            }
            _ => match crate::modrinth::fetch_latest_version(client, &project_id).await {
                Ok(Some((version_id, version_number, name))) => {
                    let entry = CachedPackLatest {
                        checked_at: now_millis(),
                        version_id,
                        version_number,
                        name,
                    };
                    cache.projects.insert(project_id.clone(), entry.clone());
                    dirty = true;
                    entry
                }
                // Нет версий у проекта или сеть недоступна — пропускаем.
                Ok(None) | Err(_) => continue,
            },
        };
        result.push(PackUpdateInfo {
            instance_id: instance.id.clone(),
            project_id,
            current_version_id: manifest.version_id.clone(),
            latest_version_id: entry.version_id.clone(),
            latest_version_number: entry.version_number.clone(),
            latest_name: entry.name.clone(),
            has_update: entry.version_id != manifest.version_id,
        });
    }

    if dirty {
        write_cache(&cache_path, &cache);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_plain_versions() {
        assert!(version_gt("0.2.0", "0.1.0"));
        assert!(!version_gt("0.1.0", "0.2.0"));
        assert!(!version_gt("0.2.0", "0.2.0"));
    }

    #[test]
    fn handles_v_prefix_and_lengths() {
        assert!(version_gt("v0.2.0", "0.1.0"));
        assert!(version_gt("0.10.0", "0.9.9"));
        assert!(version_gt("1.0", "0.9"));
        assert!(!version_gt("0.1", "0.1.1"));
    }

    #[test]
    fn handles_prerelease_suffixes() {
        assert!(version_gt("v0.2.0-beta", "0.1.0"));
        assert!(!version_gt("v0.2.0-beta", "0.2.0"));
    }

    /// Живой тест: GitHub доступен, свежий релиз парсится, текущая версия подставляется.
    #[tokio::test]
    #[ignore]
    async fn fetches_latest_release_from_github() {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Bubanya/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap();
        let latest = fetch_latest_release(&client).await.unwrap();
        assert!(latest.tag.starts_with('v'), "tag: {}", latest.tag);
        assert!(latest.url.starts_with("https://github.com/"));
    }

    /// Живой тест: модпак с устаревшей версией получает has_update = true,
    /// а результат кэшируется и не ходит в сеть при повторной проверке.
    #[tokio::test]
    #[ignore]
    async fn detects_stale_modpack_and_uses_cache() {
        let _guard = crate::paths::DATA_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir =
            std::env::temp_dir().join(format!("bubanya-updates-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("instances").join("pack-test")).unwrap();
        unsafe { std::env::set_var("BUBANYA_DATA_DIR", &dir) };

        // projectId любого реального проекта Modrinth (JEI) — проверке всё равно,
        // мод это или модпак: она просто спрашивает свежую версию проекта.
        let client = reqwest::Client::new();
        let project: serde_json::Value = client
            .get("https://api.modrinth.com/v2/project/jei")
            .header("User-Agent", "Bubanya/test")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let project_id = project["id"].as_str().unwrap().to_string();

        std::fs::write(
            dir.join("instances")
                .join("pack-test")
                .join("pack.json"),
            serde_json::json!({
                "projectId": project_id,
                "versionId": "00000000-dead-beef-0000-000000000000",
                "name": "old",
                "files": []
            })
            .to_string(),
        )
        .unwrap();

        let instance = Instance {
            id: "pack-test".to_string(),
            name: "Pack Test".to_string(),
            version_id: "1.20.1".to_string(),
            loader: "fabric".to_string(),
            loader_version: None,
            created_at: 0,
            last_played: None,
            installed: true,
            icon: None,
            ram_mb: None,
            java_path: None,
            jvm_args: None,
        };

        let result = check_pack_updates(&client, &[instance.clone()]).await;
        assert_eq!(result.len(), 1);
        assert!(result[0].has_update);
        assert!(!result[0].latest_version_id.is_empty());

        // Повторная проверка берётся из кэша: результат тот же, сеть не нужна.
        let cached = check_pack_updates(&client, &[instance]).await;
        assert_eq!(cached[0].latest_version_id, result[0].latest_version_id);

        unsafe { std::env::remove_var("BUBANYA_DATA_DIR") };
        std::fs::remove_dir_all(&dir).ok();
    }
}
