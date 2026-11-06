//! Encrypted on-device backup of notes and the People overlay.
//! The file is AES-256-GCM. The key is PBKDF2-HMAC-SHA256 of a passphrase.
//! Voice keys are omitted unless the user opts in on export and again on restore.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::{
    actions::Note,
    people::Person,
};

const MAGIC: &[u8] = b"GVB1";
const ITERATIONS: u32 = 210_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupKey {
    pub id: String,
    pub provider: String,
    pub secret: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub kind: String,
    pub version: u32,
    pub notes: Vec<Note>,
    pub people: Vec<Person>,
    #[serde(default)]
    pub keys: Vec<BackupKey>,
    /// Absent in backups written before on-device memory. Restore leaves the phone's memory alone then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<crate::local_memory::Bundle>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub notes: usize,
    pub people: usize,
    pub reminders: usize,
    pub keys: usize,
    #[serde(default)]
    pub facts: usize,
    #[serde(default)]
    pub sessions: usize,
    /// False for a backup that has no memory section.
    #[serde(default)]
    pub has_memory: bool,
}

impl Payload {
    pub fn notes_and_people(notes: Vec<Note>, people: Vec<Person>) -> Self {
        Self { kind: "grok-voice-backup".into(), version: 1, notes, people, keys: Vec::new(), memory: None }
    }

    pub fn preview(&self) -> Preview {
        Preview {
            notes: self.notes.len(),
            people: self.people.len(),
            reminders: self.people.iter().map(|p| p.reminders.len()).sum(),
            keys: self.keys.len(),
            facts: self.memory.as_ref().map(|bundle| bundle.facts.len()).unwrap_or(0),
            sessions: self.memory.as_ref().map(|bundle| bundle.sessions.len()).unwrap_or(0),
            has_memory: self.memory.is_some(),
        }
    }
}

pub fn seal(payload: &Payload, passphrase: &str) -> Result<Vec<u8>, String> {
    check_passphrase(passphrase)?;
    if payload.kind != "grok-voice-backup" || payload.version != 1 {
        return Err("This backup is not a Grok Voice file.".into());
    }
    let plain = serde_json::to_vec(payload).map_err(|e| e.to_string())?;
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt, ITERATIONS, &mut key_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let cipher_text = cipher.encrypt(nonce, plain.as_ref()).map_err(|_| "Could not encrypt the backup.".to_string())?;
    key_bytes.zeroize();
    let mut out = Vec::with_capacity(4 + 4 + 16 + 12 + cipher_text.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ITERATIONS.to_le_bytes());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&cipher_text);
    Ok(out)
}

pub fn open(bytes: &[u8], passphrase: &str) -> Result<Payload, String> {
    check_passphrase(passphrase)?;
    if bytes.len() < 4 + 4 + 16 + 12 + 16 || &bytes[..4] != MAGIC {
        return Err("That file is not a Grok Voice backup.".into());
    }
    let iterations = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if iterations < 100_000 || iterations > 1_000_000 {
        return Err("That backup uses an unsupported key strength.".into());
    }
    let salt = &bytes[8..24];
    let nonce_bytes = &bytes[24..36];
    let cipher_text = &bytes[36..];
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, iterations, &mut key_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = cipher.decrypt(nonce, cipher_text).map_err(|_| {
        key_bytes.zeroize();
        "That passphrase did not open this backup.".to_string()
    })?;
    key_bytes.zeroize();
    let payload: Payload = serde_json::from_slice(&plain).map_err(|_| "The backup opened, but its contents were unreadable.".to_string())?;
    if payload.kind != "grok-voice-backup" || payload.version != 1 {
        return Err("This backup is from a newer version.".into());
    }
    Ok(payload)
}

#[cfg(test)]
pub fn seal_b64(payload: &Payload, passphrase: &str) -> Result<String, String> {
    Ok(B64.encode(seal(payload, passphrase)?))
}

pub fn open_b64(encoded: &str, passphrase: &str) -> Result<Payload, String> {
    let bytes = B64.decode(encoded.trim()).map_err(|_| "The backup file was not readable.".to_string())?;
    open(&bytes, passphrase)
}

pub fn acceptable_key(key: &BackupKey) -> bool {
    let provider_ok = matches!(key.provider.as_str(), "xai" | "openai" | "local");
    let id_ok = !key.id.is_empty()
        && key.id.len() <= 64
        && key.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let secret = key.secret.trim();
    let secret_ok = !secret.is_empty()
        && secret.len() <= 300
        && !secret.chars().any(|c| c.is_whitespace() || c.is_control())
        && secret == key.secret;
    provider_ok && id_ok && secret_ok
}

fn check_passphrase(passphrase: &str) -> Result<(), String> {
    if passphrase.chars().count() < 8 {
        Err("Use a passphrase of at least 8 characters.".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::people::{Handles, Person, PersonReminder};

    fn sample() -> Payload {
        let note = Note { id: "n1".into(), title: "Milk".into(), text: "buy milk".into(), created_ms: 5 };
        let person = Person {
            id: "p1".into(),
            android_contact_id: None,
            display_name: "Sam Rivera".into(),
            nickname: "Mom".into(),
            aliases: vec!["mummy".into()],
            preferred_channel: "whatsapp".into(),
            handles: Handles::default(),
            favorite: true,
            note: "call after 5".into(),
            reach_out_days: 0,
            last_contacted_ms: None,
            reminders: vec![PersonReminder {
                id: "r1".into(),
                label: "birthday".into(),
                title: String::new(),
                date: "03-03".into(),
                recurrence: "yearly".into(),
                lead_days: 1,
                time: "09:00".into(),
                enabled: true,
            }],
        };
        Payload::notes_and_people(vec![note], vec![person])
    }

    #[test]
    fn round_trip_keeps_notes_people_and_drops_keys_by_default() {
        let payload = sample();
        assert!(payload.keys.is_empty());
        let bytes = seal(&payload, "correct horse").unwrap();
        let opened = open(&bytes, "correct horse").unwrap();
        assert_eq!(opened.notes[0].text, "buy milk");
        assert_eq!(opened.people[0].nickname, "Mom");
        assert_eq!(opened.people[0].reminders[0].date, "03-03");
        assert!(opened.keys.is_empty());
        assert_eq!(opened.preview().reminders, 1);
        assert!(open(&bytes, "wrong passphrase").is_err());
        assert!(seal(&payload, "short").is_err());
    }

    #[test]
    fn keys_round_trip_only_when_present() {
        let mut payload = sample();
        payload.keys.push(BackupKey { id: "k1".into(), provider: "xai".into(), secret: "xai-test-secret".into() });
        let encoded = seal_b64(&payload, "another secret").unwrap();
        let opened = open_b64(&encoded, "another secret").unwrap();
        assert_eq!(opened.keys.len(), 1);
        assert_eq!(opened.keys[0].secret, "xai-test-secret");
        assert_eq!(opened.preview().keys, 1);
    }

    #[test]
    fn old_backups_without_memory_still_open() {
        let old = r#"{"kind":"grok-voice-backup","version":1,"notes":[],"people":[],"keys":[]}"#;
        let payload: Payload = serde_json::from_str(old).unwrap();
        assert!(payload.memory.is_none());
        assert!(!payload.preview().has_memory);
        let bytes = seal(&payload, "correct horse").unwrap();
        let opened = open(&bytes, "correct horse").unwrap();
        assert!(opened.memory.is_none());
    }

    #[test]
    fn memory_round_trips_inside_version_one() {
        let mut payload = sample();
        payload.memory = Some(crate::local_memory::Bundle {
            facts: vec![crate::local_memory::Fact { id: "f1".into(), text: "I live in Zurich".into(), updated_ms: 1 }],
            sessions: vec![crate::local_memory::SessionNote { id: "s1".into(), at_ms: 2, text: "You said hello. The reply was hi.".into() }],
            next_id: 2,
        });
        let opened = open(&seal(&payload, "correct horse").unwrap(), "correct horse").unwrap();
        assert_eq!(opened.preview().facts, 1);
        assert!(opened.preview().has_memory);
        assert_eq!(opened.memory.as_ref().unwrap().facts[0].text, "I live in Zurich");
    }

    #[test]
    fn restore_rejects_unknown_providers_and_whitespace_secrets() {
        assert!(acceptable_key(&BackupKey { id: "xai-1".into(), provider: "xai".into(), secret: "xai-test-secret".into() }));
        assert!(acceptable_key(&BackupKey { id: "legacy-xai".into(), provider: "xai".into(), secret: "xai-test-secret".into() }));
        assert!(!acceptable_key(&BackupKey { id: "k1".into(), provider: "other".into(), secret: "xai-test-secret".into() }));
        assert!(!acceptable_key(&BackupKey { id: "k 1".into(), provider: "xai".into(), secret: "xai-test-secret".into() }));
        assert!(!acceptable_key(&BackupKey { id: "k1".into(), provider: "xai".into(), secret: "has space".into() }));
    }
}
