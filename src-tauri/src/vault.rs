//! On-device secret storage.
//!
//! The xAI API key lives inside an IOTA Stronghold snapshot (`vault.hold`). The snapshot's
//! 32-byte encryption key is random, and on Android it is only ever persisted wrapped
//! (AES-256-GCM) by a non-exportable Android Keystore key (StrongBox / TEE when available).
//! Nothing ever leaves the device, and the key is never handed to the WebView.

use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use rand::RngCore;
use serde::Serialize;
use tauri_plugin_stronghold::stronghold::Stronghold;
use zeroize::Zeroizing;

use crate::{
    error::{Error, Result},
    native::NativeBridge,
};

const CLIENT: &[u8] = b"grok-voice";
const API_KEY: &[u8] = b"xai_api_key";
const ACCOUNTS: &[u8] = b"voice_accounts";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub has_key: bool,
    pub key_hint: Option<String>,
    pub security_level: Option<String>,
    pub accounts: Vec<VoiceAccount>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredAccount {
    id: String,
    provider: String,
    secret: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAccount {
    pub id: String,
    pub provider: String,
    pub hint: String,
}

#[derive(Clone)]
pub struct VoiceSecret {
    pub id: String,
    pub provider: String,
    pub secret: String,
}

pub struct Vault {
    dir: PathBuf,
    inner: Mutex<Option<Stronghold>>,
    security_level: Mutex<Option<String>>,
}

fn verr(e: impl std::fmt::Display) -> Error {
    Error::Vault(e.to_string())
}

impl Vault {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, inner: Mutex::new(None), security_level: Mutex::new(None) }
    }

    fn snapshot_path(&self) -> PathBuf {
        self.dir.join("vault.hold")
    }

    fn wrapped_key_path(&self) -> PathBuf {
        self.dir.join("vault.key.wrapped")
    }

    fn reset_files(&self) {
        let _ = fs::remove_file(self.snapshot_path());
        let _ = fs::remove_file(self.wrapped_key_path());
    }

    fn create_vault_key(&self, bridge: &NativeBridge) -> Result<Zeroizing<Vec<u8>>> {
        let mut raw = Zeroizing::new(vec![0u8; 32]);
        rand::rngs::OsRng.fill_bytes(&mut raw);
        let wrapped = bridge.wrap_key(&raw)?;
        write_private(&self.wrapped_key_path(), &wrapped.blob)?;
        *self.security_level.lock().unwrap() = Some(wrapped.security_level);
        Ok(raw)
    }

    fn open(&self, bridge: &NativeBridge) -> Result<MutexGuard<'_, Option<Stronghold>>> {
        let mut guard = self.inner.lock().unwrap();
        if guard.is_some() {
            return Ok(guard);
        }
        fs::create_dir_all(&self.dir)?;

        let key = match fs::read(self.wrapped_key_path()) {
            Ok(blob) => match bridge.unwrap_key(&blob) {
                Ok(k) => {
                    if let Ok(level) = bridge.key_security_level() {
                        *self.security_level.lock().unwrap() = Some(level);
                    }
                    Zeroizing::new(k)
                }
                Err(e) => {
                    // Keystore key is gone (app data restored onto another device, or the
                    // user cleared credentials). The snapshot is unrecoverable by design.
                    log::warn!("vault key unwrap failed, resetting vault: {e}");
                    self.reset_files();
                    self.create_vault_key(bridge)?
                }
            },
            Err(_) => {
                let _ = fs::remove_file(self.snapshot_path());
                self.create_vault_key(bridge)?
            }
        };

        let stronghold = match Stronghold::new(self.snapshot_path(), key.to_vec()) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("stronghold snapshot unreadable, resetting: {e}");
                self.reset_files();
                let key = self.create_vault_key(bridge)?;
                Stronghold::new(self.snapshot_path(), key.to_vec()).map_err(verr)?
            }
        };
        if stronghold.load_client(CLIENT).is_err() {
            stronghold.create_client(CLIENT).map_err(verr)?;
        }
        *guard = Some(stronghold);
        Ok(guard)
    }

    fn with_store<T>(
        &self,
        bridge: &NativeBridge,
        f: impl FnOnce(&Stronghold, &iota_stronghold::Client) -> Result<T>,
    ) -> Result<T> {
        let guard = self.open(bridge)?;
        let sh = guard.as_ref().expect("vault opened");
        let client = sh.get_client(CLIENT).map_err(verr)?;
        f(sh, &client)
    }

    pub fn set_api_key(&self, bridge: &NativeBridge, key: &str) -> Result<()> {
        self.with_store(bridge, |sh, client| {
            client
                .store()
                .insert(API_KEY.to_vec(), key.as_bytes().to_vec(), None)
                .map_err(verr)?;
            sh.write_client(CLIENT).map_err(verr)?;
            sh.save().map_err(verr)
        })
    }

    pub fn api_key(&self, bridge: &NativeBridge) -> Result<Option<Zeroizing<String>>> {
        self.with_store(bridge, |_, client| {
            let bytes = client.store().get(API_KEY).map_err(verr)?;
            Ok(bytes.map(|b| Zeroizing::new(String::from_utf8_lossy(&b).into_owned())))
        })
    }

    pub fn delete_api_key(&self, bridge: &NativeBridge) -> Result<()> {
        self.with_store(bridge, |sh, client| {
            client.store().delete(API_KEY).map_err(verr)?;
            sh.write_client(CLIENT).map_err(verr)?;
            sh.save().map_err(verr)
        })
    }

    pub fn accounts(&self, bridge: &NativeBridge) -> Result<Vec<VoiceSecret>> {
        self.with_store(bridge, |_, client| {
            let mut list = read_accounts(client)?;
            if list.is_empty() {
                if let Some(bytes) = client.store().get(API_KEY).map_err(verr)? {
                    let secret = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !secret.is_empty() {
                        list.push(StoredAccount {
                            id: "legacy-xai".into(),
                            provider: "xai".into(),
                            secret,
                        });
                    }
                }
            }
            Ok(list
                .into_iter()
                .map(|a| VoiceSecret { id: a.id, provider: a.provider, secret: a.secret })
                .collect())
        })
    }

    pub fn add_account(&self, bridge: &NativeBridge, provider: &str, secret: &str) -> Result<Vec<VoiceAccount>> {
        let provider = provider.trim().to_string();
        let secret = secret.trim().to_string();
        self.with_store(bridge, |sh, client| {
            let mut list = read_accounts(client)?;
            if list.is_empty() {
                if let Some(bytes) = client.store().get(API_KEY).map_err(verr)? {
                    let legacy = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !legacy.is_empty() {
                        list.push(StoredAccount {
                            id: "legacy-xai".into(),
                            provider: "xai".into(),
                            secret: legacy,
                        });
                    }
                }
            }
            if provider == "local" && list.iter().any(|account| account.provider == "local") {
                return Err(crate::error::msg("On device is already in the list."));
            }
            list.push(StoredAccount {
                id: format!("{provider}-{}", chrono::Utc::now().timestamp_millis()),
                provider,
                secret,
            });
            write_accounts(sh, client, &list)?;
            Ok(public_accounts(&list))
        })
    }

    pub fn remove_account(&self, bridge: &NativeBridge, id: &str) -> Result<Vec<VoiceAccount>> {
        self.with_store(bridge, |sh, client| {
            let mut list = read_accounts(client)?;
            if list.is_empty() {
                if let Some(bytes) = client.store().get(API_KEY).map_err(verr)? {
                    let legacy = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !legacy.is_empty() {
                        list.push(StoredAccount { id: "legacy-xai".into(), provider: "xai".into(), secret: legacy });
                    }
                }
            }
            list.retain(|a| a.id != id);
            write_accounts(sh, client, &list)?;
            if list.is_empty() {
                let _ = client.store().delete(API_KEY);
                sh.write_client(CLIENT).map_err(verr)?;
                sh.save().map_err(verr)?;
            }
            Ok(public_accounts(&list))
        })
    }

    pub fn update_account(&self, bridge: &NativeBridge, id: &str, provider: &str, secret: &str) -> Result<Vec<VoiceAccount>> {
        let provider = provider.trim().to_string();
        let secret = secret.trim().to_string();
        self.with_store(bridge, |sh, client| {
            let mut list = read_accounts(client)?;
            if list.is_empty() {
                if let Some(bytes) = client.store().get(API_KEY).map_err(verr)? {
                    let legacy = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !legacy.is_empty() {
                        list.push(StoredAccount { id: "legacy-xai".into(), provider: "xai".into(), secret: legacy });
                    }
                }
            }
            let Some(index) = list.iter().position(|account| account.id == id) else {
                return Err(crate::error::msg("That voice key is no longer on this phone."));
            };
            if provider == "local" && list.iter().any(|account| account.provider == "local" && account.id != id) {
                return Err(crate::error::msg("On device is already in the list."));
            }
            if provider != "local" && secret.is_empty() && provider != list[index].provider {
                return Err(crate::error::msg("Paste the key for this provider."));
            }
            let next_secret = if provider == "local" {
                "on-device".to_string()
            } else if secret.is_empty() {
                list[index].secret.clone()
            } else {
                secret
            };
            list[index].provider = provider;
            list[index].secret = next_secret;
            write_accounts(sh, client, &list)?;
            Ok(public_accounts(&list))
        })
    }

    pub fn move_account(&self, bridge: &NativeBridge, id: &str, up: bool) -> Result<Vec<VoiceAccount>> {
        self.with_store(bridge, |sh, client| {
            let mut list = read_accounts(client)?;
            if list.is_empty() {
                if let Some(bytes) = client.store().get(API_KEY).map_err(verr)? {
                    let legacy = String::from_utf8_lossy(&bytes).trim().to_string();
                    if !legacy.is_empty() {
                        list.push(StoredAccount { id: "legacy-xai".into(), provider: "xai".into(), secret: legacy });
                    }
                }
            }
            if let Some(i) = list.iter().position(|a| a.id == id) {
                let j = if up { i.saturating_sub(1) } else { (i + 1).min(list.len().saturating_sub(1)) };
                if i != j {
                    list.swap(i, j);
                }
            }
            write_accounts(sh, client, &list)?;
            Ok(public_accounts(&list))
        })
    }

    /// Replaces the vault account list. Used only when a backup restore explicitly opts in.
    pub fn replace_secrets(&self, bridge: &NativeBridge, rows: &[(String, String, String)]) -> Result<()> {
        let list: Vec<StoredAccount> = rows
            .iter()
            .filter(|(_, _, secret)| !secret.trim().is_empty())
            .map(|(id, provider, secret)| StoredAccount {
                id: id.clone(),
                provider: provider.trim().to_string(),
                secret: secret.trim().to_string(),
            })
            .collect();
        self.with_store(bridge, |sh, client| {
            write_accounts(sh, client, &list)?;
            if list.is_empty() {
                let _ = client.store().delete(API_KEY);
                sh.write_client(CLIENT).map_err(verr)?;
                sh.save().map_err(verr)?;
            }
            Ok(())
        })
    }

    pub fn status(&self, bridge: &NativeBridge) -> Result<AuthStatus> {
        let accounts = self.accounts(bridge)?;
        let first = accounts.iter().find(|a| a.provider == "xai").or(accounts.first());
        Ok(AuthStatus {
            has_key: !accounts.is_empty(),
            key_hint: first.map(|a| key_hint(&a.secret)),
            security_level: self.security_level.lock().unwrap().clone(),
            accounts: accounts
                .iter()
                .map(|a| VoiceAccount {
                    id: a.id.clone(),
                    provider: a.provider.clone(),
                    hint: key_hint(&a.secret),
                })
                .collect(),
        })
    }

    pub fn security_level(&self) -> Option<String> {
        self.security_level.lock().unwrap().clone()
    }
}

fn read_accounts(client: &iota_stronghold::Client) -> Result<Vec<StoredAccount>> {
    let Some(bytes) = client.store().get(ACCOUNTS).map_err(verr)? else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

fn write_accounts(sh: &Stronghold, client: &iota_stronghold::Client, list: &[StoredAccount]) -> Result<()> {
    let bytes = serde_json::to_vec(list).map_err(|e| verr(e.to_string()))?;
    client.store().insert(ACCOUNTS.to_vec(), bytes, None).map_err(verr)?;
    if let Some(xai) = list.iter().find(|a| a.provider == "xai") {
        client
            .store()
            .insert(API_KEY.to_vec(), xai.secret.as_bytes().to_vec(), None)
            .map_err(verr)?;
    }
    sh.write_client(CLIENT).map_err(verr)?;
    sh.save().map_err(verr)
}

fn public_accounts(list: &[StoredAccount]) -> Vec<VoiceAccount> {
    list.iter()
        .map(|a| VoiceAccount {
            id: a.id.clone(),
            provider: a.provider.clone(),
            hint: key_hint(&a.secret),
        })
        .collect()
}

pub fn key_hint(key: &str) -> String {
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let head: String = key.chars().take(4).collect();
    format!("{head}…{tail}")
}

fn write_private(path: &PathBuf, data: &[u8]) -> Result<()> {
    fs::write(path, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
