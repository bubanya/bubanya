#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

const SERVICE: &str = "bubanya";

/// Хранит refresh token в системном keyring (GNOME Keyring / KWallet через secret-tool).
/// В файлы токены не пишутся.
pub fn store_refresh(account_id: &str, token: &str) {
    #[cfg(target_os = "linux")]
    {
        use std::io::Write;

        let label = format!("Bubanya Microsoft {account_id}");
        let Ok(mut child) = Command::new("secret-tool")
            .args([
                "store", "--label", &label, "service", SERVICE, "account", account_id,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(token.as_bytes());
        }
        let _ = child.wait();
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(entry) = keyring::Entry::new(SERVICE, account_id) {
            let _ = entry.set_password(token);
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = (account_id, token);
    }
}

pub fn load_refresh(account_id: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("secret-tool")
            .args(["lookup", "service", SERVICE, "account", account_id])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() { None } else { Some(token) }
    }
    #[cfg(target_os = "windows")]
    {
        let entry = keyring::Entry::new(SERVICE, account_id).ok()?;
        let token = entry.get_password().ok()?;
        if token.is_empty() { None } else { Some(token) }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = account_id;
        None
    }
}

pub fn delete_refresh(account_id: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("secret-tool")
            .args(["clear", "service", SERVICE, "account", account_id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(entry) = keyring::Entry::new(SERVICE, account_id) {
            let _ = entry.delete_credential();
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = account_id;
    }
}
