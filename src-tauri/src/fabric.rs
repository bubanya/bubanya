use crate::error::{Error, Result};
use crate::model::FabricLoader;
use crate::mojang::{Arguments, Library, VersionJson};
use crate::paths;
use serde::Deserialize;

const META: &str = "https://meta.fabricmc.net/v2/versions/loader";

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[derive(Debug, Deserialize)]
pub struct FabricProfile {
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    libraries: Vec<Library>,
    #[serde(default)]
    arguments: Option<Arguments>,
}

/// Все загрузчики Fabric, доступные для конкретной версии игры.
pub async fn fetch_loaders(client: &reqwest::Client, game: &str) -> Result<Vec<FabricLoader>> {
    let url = format!("{META}/{game}");
    let entries: Vec<LoaderEntry> = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(entries
        .into_iter()
        .map(|entry| FabricLoader {
            version: entry.loader.version,
            stable: entry.loader.stable,
        })
        .collect())
}

/// Последний стабильный загрузчик (или первый доступный).
pub async fn latest_loader(client: &reqwest::Client, game: &str) -> Result<String> {
    let loaders = fetch_loaders(client, game).await?;
    if let Some(stable) = loaders.iter().find(|loader| loader.stable) {
        return Ok(stable.version.clone());
    }
    loaders
        .first()
        .map(|loader| loader.version.clone())
        .ok_or_else(|| Error::Msg(format!("Для {game} нет Fabric-загрузчиков")))
}

/// Профиль Fabric кэшируется в `meta/fabric/<game>-<loader>.json`.
pub async fn fetch_profile(
    client: &reqwest::Client,
    game: &str,
    loader: &str,
) -> Result<FabricProfile> {
    let cache_dir = paths::meta().join("fabric");
    let cache = cache_dir.join(format!("{game}-{loader}.json"));
    if cache.exists()
        && let Ok(text) = std::fs::read_to_string(&cache)
        && let Ok(profile) = serde_json::from_str(&text)
    {
        return Ok(profile);
    }
    let url = format!("{META}/{game}/{loader}/profile/json");
    let text = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    std::fs::create_dir_all(&cache_dir)?;
    std::fs::write(&cache, &text)?;
    Ok(serde_json::from_str(&text)?)
}

/// Сливает ванильный JSON и Fabric-профиль: mainClass, библиотеки и аргументы.
pub fn merge(mut vanilla: VersionJson, profile: FabricProfile) -> VersionJson {
    vanilla.main_class = profile.main_class;
    vanilla.libraries.extend(profile.libraries);
    if let Some(mut fabric_arguments) = profile.arguments {
        if let Some(arguments) = vanilla.arguments.as_mut() {
            arguments.jvm.append(&mut fabric_arguments.jvm);
            arguments.game.append(&mut fabric_arguments.game);
        } else {
            vanilla.arguments = Some(fabric_arguments);
        }
    }
    vanilla
}
