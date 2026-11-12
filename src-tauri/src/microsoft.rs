use crate::error::{Error, Result};
use crate::model::{Account, Accounts};
use crate::token_store;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

pub const CLIENT_ID: &str = "00000000402b5328";
const SCOPE: &str = "XboxLive.signin XboxLive.offline_access";
const LIVE_BASE: &str = "https://login.live.com";
const USER_AUTH: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeInfo {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub interval: u64,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    expires_in: u64,
    #[serde(default)]
    interval: u64,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollResult {
    pub status: String,
    pub message: String,
    pub accounts: Option<Accounts>,
}

#[derive(Debug, Clone)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct MinecraftAuth {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub xuid: String,
    pub profile: MinecraftProfile,
}

pub enum PollOutcome {
    Pending,
    Error(String),
    Success(OAuthToken),
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

async fn send_retry<F>(build: F) -> Result<reqwest::Response>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    let mut last_error = None;
    for attempt in 1..=4u32 {
        match build().send().await {
            Ok(response) if response.status().is_server_error() && attempt < 4 => {
                last_error = Some(Error::Msg(format!("Xbox/HTTP {}", response.status())));
            }
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = Some(Error::Http(error));
            }
        }
        tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
    }
    Err(last_error.unwrap_or_else(|| Error::Msg("Сеть недоступна".to_string())))
}

fn parse_oauth(value: &Value) -> Result<OAuthToken> {
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("Microsoft не вернул access token".to_string()))?
        .to_string();
    let refresh_token = value
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(OAuthToken {
        access_token,
        refresh_token,
    })
}

pub async fn begin(client: &reqwest::Client) -> Result<DeviceCodeInfo> {
    let raw: DeviceCodeResponse = client
        .post(format!("{LIVE_BASE}/oauth20_connect.srf"))
        .form(&[
            ("client_id", CLIENT_ID),
            ("scope", SCOPE),
            ("response_type", "device_code"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(DeviceCodeInfo {
        device_code: raw.device_code,
        user_code: raw.user_code,
        verification_uri: raw.verification_uri,
        expires_in: raw.expires_in,
        interval: raw.interval,
        message: raw.message,
    })
}

pub async fn poll_token(client: &reqwest::Client, device_code: &str) -> Result<PollOutcome> {
    let response = client
        .post(format!("{LIVE_BASE}/oauth20_token.srf"))
        .form(&[
            ("client_id", CLIENT_ID),
            ("grant_type", "device_code"),
            ("device_code", device_code),
        ])
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| Error::Msg(format!("Microsoft: неожиданный ответ (HTTP {status})")))?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        if error == "authorization_pending" || error == "slow_down" {
            return Ok(PollOutcome::Pending);
        }
        let message = value
            .get("error_description")
            .and_then(Value::as_str)
            .unwrap_or(error)
            .to_string();
        return Ok(PollOutcome::Error(message));
    }
    if !status.is_success() {
        return Ok(PollOutcome::Error(format!(
            "Microsoft вернул HTTP {status}"
        )));
    }
    Ok(PollOutcome::Success(parse_oauth(&value)?))
}

pub async fn refresh_token(client: &reqwest::Client, refresh: &str) -> Result<OAuthToken> {
    let response = send_retry(|| {
        client
            .post(format!("{LIVE_BASE}/oauth20_token.srf"))
            .form(&[
                ("client_id", CLIENT_ID),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("scope", SCOPE),
            ])
    })
    .await?;
    let status = response.status();
    let text = response.text().await?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| Error::Msg(format!("Microsoft: неожиданный ответ (HTTP {status})")))?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        let message = value
            .get("error_description")
            .and_then(Value::as_str)
            .unwrap_or(error);
        return Err(Error::Msg(format!(
            "Не удалось обновить вход Microsoft: {message}"
        )));
    }
    if !status.is_success() {
        return Err(Error::Msg(format!(
            "Не удалось обновить вход Microsoft: HTTP {status}"
        )));
    }
    parse_oauth(&value)
}

pub async fn complete(client: &reqwest::Client, oauth: OAuthToken) -> Result<MinecraftAuth> {
    let refresh = oauth
        .refresh_token
        .ok_or_else(|| Error::Msg("Microsoft не выдал refresh token".to_string()))?;

    let xbl_body = json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", oauth.access_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let xbl: Value = send_retry(|| client.post(USER_AUTH).json(&xbl_body))
        .await?
        .error_for_status()?
        .json()
        .await?;
    let xbl_token = xbl
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("Xbox не вернул токен".to_string()))?
        .to_string();

    let xsts_body = json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });
    let xsts_response = send_retry(|| client.post(XSTS_AUTH).json(&xsts_body)).await?;
    let xsts_status = xsts_response.status();
    let xsts_text = xsts_response.text().await?;
    let xsts: Value = serde_json::from_str(&xsts_text)
        .map_err(|_| Error::Msg(format!("Xbox: неожиданный ответ (HTTP {xsts_status})")))?;
    if !xsts_status.is_success() {
        let message = xsts
            .get("XErr")
            .and_then(Value::as_u64)
            .map(|code| match code {
                2148916233 => "У аккаунта нет Xbox-профиля. Зайди на xbox.com и создай его.",
                2148916235 => "Xbox Live недоступен в регионе аккаунта.",
                2148916238 => "Аккаунт ребёнка нужно добавить в семейную группу Microsoft.",
                _ => "Xbox отклонил вход.",
            })
            .unwrap_or("Xbox отклонил вход.");
        return Err(Error::Msg(message.to_string()));
    }
    let xsts_token = xsts
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("XSTS не вернул токен".to_string()))?
        .to_string();
    let uhs = xsts
        .pointer("/DisplayClaims/xui/0/uhs")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("XSTS не вернул UHS".to_string()))?
        .to_string();
    let xuid = xsts
        .pointer("/DisplayClaims/xui/0/xid")
        .and_then(Value::as_str)
        .unwrap_or("0")
        .to_string();

    let login_body = json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
    });
    let minecraft: Value = send_retry(|| client.post(MC_LOGIN).json(&login_body))
        .await?
        .error_for_status()?
        .json()
        .await?;
    let access_token = minecraft
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("Minecraft не вернул access token".to_string()))?
        .to_string();
    let expires_in = minecraft
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(86400);

    let profile: Value = send_retry(|| client.get(MC_PROFILE).bearer_auth(&access_token))
        .await?
        .error_for_status()?
        .json()
        .await?;
    let id = profile
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("Minecraft не вернул профиль".to_string()))?
        .to_string();
    let name = profile
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Msg("Minecraft не вернул ник".to_string()))?
        .to_string();

    Ok(MinecraftAuth {
        access_token,
        refresh_token: refresh,
        expires_at: now_millis() + (expires_in as i64) * 1000 - 60_000,
        xuid,
        profile: MinecraftProfile { id, name },
    })
}

pub async fn ensure_token(
    client: &reqwest::Client,
    account: &Account,
    cached: Option<MinecraftAuth>,
) -> Result<MinecraftAuth> {
    if let Some(auth) = &cached
        && auth.expires_at > now_millis() + 60_000
    {
        return Ok(auth.clone());
    }
    let refresh = cached
        .as_ref()
        .map(|auth| auth.refresh_token.clone())
        .or_else(|| token_store::load_refresh(&account.id))
        .ok_or_else(|| Error::Msg("Вход Microsoft истёк. Подключи аккаунт заново.".to_string()))?;
    let oauth = refresh_token(client, &refresh).await?;
    let auth = complete(client, oauth).await?;
    token_store::store_refresh(&account.id, &auth.refresh_token);
    Ok(auth)
}

pub fn dashed_uuid(id: &str) -> String {
    let clean: String = id
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .collect();
    if clean.len() != 32 {
        return id.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &clean[0..8],
        &clean[8..12],
        &clean[12..16],
        &clean[16..20],
        &clean[20..32]
    )
}

pub fn make_account(auth: &MinecraftAuth, existing: Option<&Account>) -> Account {
    Account {
        id: existing
            .map(|account| account.id.clone())
            .unwrap_or_else(|| dashed_uuid(&auth.profile.id)),
        kind: "microsoft".to_string(),
        name: auth.profile.name.clone(),
        uuid: dashed_uuid(&auth.profile.id),
        xuid: Some(auth.xuid.clone()),
        created_at: existing
            .map(|account| account.created_at)
            .unwrap_or_else(now_millis),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn fetches_notch_skin() {
        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let url = skin_url(&client, "069a79f4-44e9-4726-a5be-fca90e38aaf5")
            .await
            .unwrap();
        assert!(url.is_some(), "sessionserver не вернул скин");
    }

    #[tokio::test]
    #[ignore]
    async fn xbox_user_auth_reachable() {
        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let response = send_retry(|| client.post(USER_AUTH).json(&json!({})))
            .await
            .unwrap();
        assert!(
            response.status().is_client_error(),
            "Xbox вернул {}",
            response.status()
        );
    }

    #[tokio::test]
    #[ignore]
    async fn fetches_notch_head() {
        let client = reqwest::Client::builder().http1_only().build().unwrap();
        let head = skin_head(&client, "069a79f4-44e9-4726-a5be-fca90e38aaf5")
            .await
            .unwrap()
            .expect("sessionserver не вернул скин");
        assert!(head.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn dashed_uuid_formats_profile_id() {
        assert_eq!(
            dashed_uuid("0123456789abcdef0123456789abcdef"),
            "01234567-89ab-cdef-0123-456789abcdef"
        );
        assert_eq!(dashed_uuid("not-a-uuid"), "not-a-uuid");
    }

    #[test]
    fn make_account_reuses_existing_id() {
        let auth = MinecraftAuth {
            access_token: "token".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: 0,
            xuid: "123".to_string(),
            profile: MinecraftProfile {
                id: "0123456789abcdef0123456789abcdef".to_string(),
                name: "Steve".to_string(),
            },
        };
        let existing = Account {
            id: "existing".to_string(),
            kind: "microsoft".to_string(),
            name: "Old".to_string(),
            uuid: "old".to_string(),
            xuid: None,
            created_at: 42,
        };
        let account = make_account(&auth, Some(&existing));
        assert_eq!(account.id, "existing");
        assert_eq!(account.name, "Steve");
        assert_eq!(account.uuid, "01234567-89ab-cdef-0123-456789abcdef");
        assert_eq!(account.created_at, 42);
        assert_eq!(account.xuid.as_deref(), Some("123"));
    }
}

#[derive(Debug, Deserialize)]
struct SessionProfile {
    #[serde(default)]
    properties: Vec<SessionProperty>,
}

#[derive(Debug, Deserialize)]
struct SessionProperty {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct TexturePayload {
    textures: TextureMap,
}

#[derive(Debug, Deserialize)]
struct TextureMap {
    #[serde(rename = "SKIN")]
    skin: Option<TextureEntry>,
}

#[derive(Debug, Deserialize)]
struct TextureEntry {
    url: String,
}

/// Возвращает готовую голову скина (data:image/png) из официального sessionserver.
pub async fn skin_head(client: &reqwest::Client, uuid: &str) -> Result<Option<String>> {
    let Some(url) = skin_url(client, uuid).await? else {
        return Ok(None);
    };
    let response = send_retry(|| client.get(&url)).await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let bytes = response.bytes().await?;
    let image = image::load_from_memory(&bytes)?;
    if image.width() < 16 || image.height() < 16 {
        return Ok(None);
    }
    // Лицо головы: пиксели 8..16 по обеим осям и в 64x64, и в старом 64x32.
    let head = image
        .crop_imm(8, 8, 8, 8)
        .resize(64, 64, image::imageops::FilterType::Nearest);
    let mut out = std::io::Cursor::new(Vec::new());
    head.write_to(&mut out, image::ImageFormat::Png)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(out.into_inner());
    Ok(Some(format!("data:image/png;base64,{encoded}")))
}

/// Возвращает URL скина из официального sessionserver Mojang.
pub async fn skin_url(client: &reqwest::Client, uuid: &str) -> Result<Option<String>> {
    let clean: String = uuid.chars().filter(char::is_ascii_hexdigit).collect();
    if clean.len() != 32 {
        return Ok(None);
    }
    let response = send_retry(|| {
        client.get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{clean}"
        ))
    })
    .await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let profile: SessionProfile = response.json().await?;
    for property in profile.properties {
        if property.name != "textures" {
            continue;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(property.value) else {
            continue;
        };
        let Ok(payload) = serde_json::from_slice::<TexturePayload>(&bytes) else {
            continue;
        };
        if let Some(skin) = payload.textures.skin {
            return Ok(Some(skin.url.replacen("http://", "https://", 1)));
        }
    }
    Ok(None)
}
