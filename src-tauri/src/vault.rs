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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub has_key: bool,
    pub key_hint: Option<String>,
    pub security_level: Option<String>,
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

    pub fn status(&self, bridge: &NativeBridge) -> Result<AuthStatus> {
        let key = self.api_key(bridge)?;
        Ok(AuthStatus {
            has_key: key.is_some(),
            key_hint: key.as_ref().map(|k| key_hint(k)),
            security_level: self.security_level.lock().unwrap().clone(),
        })
    }

    pub fn security_level(&self) -> Option<String> {
        self.security_level.lock().unwrap().clone()
    }
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
