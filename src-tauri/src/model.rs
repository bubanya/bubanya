use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub nickname: String,
    pub ram_mb: u32,
    pub java_path: Option<String>,
    pub close_on_launch: bool,
    pub concurrency: u32,
    #[serde(default = "default_accent")]
    pub accent: String,
}

fn default_accent() -> String {
    "#E8A663".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            nickname: "Player".to_string(),
            ram_mb: 2048,
            java_path: None,
            close_on_launch: false,
            concurrency: 12,
            accent: default_accent(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub loader: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub created_at: i64,
    pub last_played: Option<i64>,
    pub installed: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub ram_mb: Option<u32>,
    #[serde(default)]
    pub java_path: Option<String>,
    #[serde(default)]
    pub jvm_args: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricLoader {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub uuid: String,
    #[serde(default)]
    pub xuid: Option<String>,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchAuth {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
    pub xuid: String,
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Accounts {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active_id: Option<String>,
}

impl Accounts {
    pub fn active(&self) -> Option<&Account> {
        let id = self.active_id.as_ref()?;
        self.accounts.iter().find(|account| &account.id == id)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeVersion {
    pub version: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    pub kind: String,
    pub release_time: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    pub path: String,
    pub version: String,
    pub major: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub instance_id: String,
    pub stage: String,
    pub current: u64,
    pub total: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchInfo {
    pub pid: u32,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub categories: Vec<String>,
    pub project_type: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub author: Option<String>,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchResult {
    pub hits: Vec<ModrinthHit>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub filename: String,
    pub size: u64,
    pub date_published: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub filename: String,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub enabled: bool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentFile {
    pub filename: String,
    pub name: String,
    pub size: u64,
    pub enabled: bool,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    pub folder: String,
    pub name: String,
    pub size: u64,
    pub last_modified: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStats {
    pub size_bytes: u64,
    pub mods: u32,
    pub worlds: u32,
}
