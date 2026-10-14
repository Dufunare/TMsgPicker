use crate::gateway::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationConfig {
    pub schema_version: u32,
    pub api_id: Option<i32>,
    pub proxy_enabled: bool,
    pub proxy_server: String,
    pub proxy_port: u16,
    pub proxy_username: String,
    pub last_profile_id: Option<String>,
    pub left_ratio: f64,
    pub right_ratio: f64,
    pub card_compact: bool,
}
impl Default for ApplicationConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            api_id: None,
            proxy_enabled: false,
            proxy_server: "127.0.0.1".into(),
            proxy_port: 1080,
            proxy_username: String::new(),
            last_profile_id: None,
            left_ratio: 0.23,
            right_ratio: 0.29,
            card_compact: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AccountProfile {
    pub profile_id: String,
    pub telegram_user_id: Option<String>,
    pub display_name: String,
    pub username: Option<String>,
    pub is_legacy_profile: bool,
    pub last_used_at: u64,
    #[serde(default)]
    pub session_directory_id: String,
    #[serde(default)]
    pub created_at: u64,
}
pub type Settings = ApplicationConfig;
pub type Profile = AccountProfile;
#[derive(Default, Serialize, Deserialize)]
struct Disk {
    settings: Settings,
    profiles: Vec<Profile>,
    legacy_imported: bool,
    secrets: BTreeMap<String, Vec<u8>>,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub settings: Settings,
    pub profiles: Vec<ProfileView>,
    pub has_saved_api_hash: bool,
    pub has_saved_proxy_password: bool,
}
#[derive(Serialize)]
pub struct ProfileView {
    #[serde(flatten)]
    pub profile: Profile,
    pub has_saved_database_secret: bool,
}
#[derive(Default, Serialize, Deserialize)]
pub struct ApplicationConfigService {
    value: ApplicationConfig,
}
impl ApplicationConfigService {
    pub fn settings(&self) -> ApplicationConfig {
        self.value.clone()
    }
    pub fn validate(settings: &Settings) -> Result<()> {
        if settings.schema_version != 1
            || settings.api_id.is_some_and(|n| n <= 0)
            || settings.proxy_port == 0
            || !settings.left_ratio.is_finite()
            || !settings.right_ratio.is_finite()
            || !(0.02..=0.9).contains(&settings.left_ratio)
            || !(0.02..=0.9).contains(&settings.right_ratio)
        {
            return Err("Invalid settings".into());
        }
        Ok(())
    }
    fn replace(&mut self, settings: ApplicationConfig) -> Result<()> {
        Self::validate(&settings)?;
        self.value = settings;
        Ok(())
    }
    fn select(&mut self, id: Option<String>) {
        self.value.last_profile_id = id;
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct AccountRegistry {
    profiles: Vec<AccountProfile>,
    legacy_imported: bool,
}
#[derive(Default, Serialize, Deserialize)]
pub struct AccountCredentialStore {
    secrets: BTreeMap<String, Vec<u8>>,
}
// Process-local credential view. Never serialize plaintext or return it through IPC.
pub struct AccountCredentials {
    pub profile_id: String,
    pub session_root: PathBuf,
    pub remember_database: bool,
    pub passphrase: Option<String>,
}
pub struct Store {
    pub root: PathBuf,
    config: ApplicationConfigService,
    registry: AccountRegistry,
    credentials: AccountCredentialStore,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).map_err(|_| "Cannot create settings directory")?;
        let path = root.join("settings.json");
        let disk: Disk = if !root.join("app-config.json").exists() && path.exists() {
            serde_json::from_slice(&fs::read(path).map_err(|_| "Cannot read settings")?)
                .map_err(|_| "Settings file is damaged; restore a backup before continuing")?
        } else {
            Disk::default()
        };
        if disk.settings.schema_version != 1 {
            return Err("Unsupported settings version".into());
        }
        let mut s = if root.join("app-config.json").exists() {
            fn read<T: serde::de::DeserializeOwned>(path: PathBuf) -> Result<T> {
                serde_json::from_slice(
                    &fs::read(path).map_err(|_| "Cannot read application storage")?,
                )
                .map_err(|_| "Application storage damaged; restore a valid backup".into())
            }
            Self {
                config: ApplicationConfigService {
                    value: read(root.join("app-config.json"))?,
                },
                registry: read(root.join("accounts.json"))?,
                credentials: read(root.join("credential-store.json"))?,
                root,
            }
        } else {
            // Keep the original settings.json as a non-destructive migration backup.
            Self {
                root,
                config: ApplicationConfigService {
                    value: disk.settings,
                },
                registry: AccountRegistry {
                    profiles: disk.profiles,
                    legacy_imported: disk.legacy_imported,
                },
                credentials: AccountCredentialStore {
                    secrets: disk.secrets,
                },
            }
        };
        if s.config.value.schema_version != 1 {
            return Err("Unsupported settings version".into());
        }
        for p in &mut s.registry.profiles {
            if p.session_directory_id.is_empty() {
                p.session_directory_id = if p.is_legacy_profile {
                    "legacy".into()
                } else {
                    p.profile_id.clone()
                };
            }
        }
        if !s.registry.legacy_imported && s.root.join("tdlib/db").exists() {
            s.registry.profiles.push(Profile {
                profile_id: "legacy".into(),
                telegram_user_id: None,
                display_name: "已有账号 (Legacy)".into(),
                username: None,
                is_legacy_profile: true,
                last_used_at: 0,
                session_directory_id: "legacy".into(),
                created_at: now(),
            });
            s.config.select(Some("legacy".into()));
            s.registry.legacy_imported = true;
        }
        let mut ids = std::collections::BTreeSet::new();
        for p in &s.registry.profiles {
            if !ids.insert(&p.profile_id) {
                return Err("Duplicate Profile ID in settings; restore a valid backup".into());
            }
            s.session_root(&p.profile_id)?;
        }
        s.save()?;
        Ok(s)
    }
    fn save(&self) -> Result<()> {
        fn write(root: &Path, name: &str, value: &impl Serialize) -> Result<()> {
            let path = root.join(name);
            let temp = root.join(format!("{name}.tmp"));
            let bytes = serde_json::to_vec_pretty(value).map_err(|_| "Cannot serialize storage")?;
            let mut f = fs::File::create(&temp).map_err(|_| "Cannot write storage")?;
            f.write_all(&bytes)
                .and_then(|_| f.sync_all())
                .map_err(|_| "Cannot flush storage")?;
            drop(f);
            replace(&temp, &path)
        }
        write(&self.root, "credential-store.json", &self.credentials)?;
        write(&self.root, "accounts.json", &self.registry)?;
        write(&self.root, "app-config.json", &self.config.value)
    }
    pub fn profile_ids(&self) -> Vec<String> {
        self.registry
            .profiles
            .iter()
            .map(|p| p.profile_id.clone())
            .collect()
    }
    pub fn account_credentials(
        &self,
        id: &str,
        input: Option<String>,
    ) -> Result<AccountCredentials> {
        let session_root = self.session_root(id)?;
        let key = format!("db:{id}");
        Ok(AccountCredentials {
            profile_id: id.into(),
            session_root,
            remember_database: self.has(&key),
            passphrase: match input.filter(|s| !s.is_empty()) {
                Some(value) => Some(value),
                None => self.secret(&key)?,
            },
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            settings: self.config.value.clone(),
            profiles: self
                .registry
                .profiles
                .iter()
                .map(|p| ProfileView {
                    profile: p.clone(),
                    has_saved_database_secret: self.has(&format!("db:{}", p.profile_id)),
                })
                .collect(),
            has_saved_api_hash: self.has("api"),
            has_saved_proxy_password: self.has("proxy"),
        }
    }
    pub fn settings(&self) -> Settings {
        self.config.settings()
    }
    pub fn validate_settings(settings: &Settings) -> Result<()> {
        ApplicationConfigService::validate(settings)
    }
    pub fn set_settings(&mut self, settings: Settings) -> Result<()> {
        Self::validate_settings(&settings)?;
        self.config.replace(settings)?;
        self.save()
    }
    pub fn edit_application_config(
        &mut self,
        settings: Settings,
        api: Option<String>,
        proxy: Option<String>,
    ) -> Result<()> {
        Self::validate_settings(&settings)?;
        let mut secrets = self.credentials.secrets.clone();
        for (key, value) in [("api", api), ("proxy", proxy)] {
            if let Some(value) = value {
                if value.is_empty() {
                    return Err("Credential cannot be empty".into());
                }
                secrets.insert(key.into(), protect(value.as_bytes(), true)?);
            }
        }
        self.credentials.secrets = secrets;
        self.config.replace(settings)?;
        self.save()
    }
    pub fn set_card_compact(&mut self, compact: bool) -> Result<()> {
        let mut settings = self.settings();
        settings.card_compact = compact;
        self.set_settings(settings)
    }
    pub fn reset(&mut self) -> Result<()> {
        self.config.replace(Settings::default())?;
        self.save()
    }
    pub fn profile(&self, id: &str) -> Result<&Profile> {
        self.registry
            .profiles
            .iter()
            .find(|p| p.profile_id == id)
            .ok_or("Unknown account".into())
    }
    pub fn session_root(&self, id: &str) -> Result<PathBuf> {
        let p = self.profile(id)?;
        if p.is_legacy_profile && id == "legacy" {
            return Ok(self.root.join("tdlib"));
        }
        if p.is_legacy_profile
            || !id.starts_with("p-")
            || !id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b'p' || b == b'-')
        {
            return Err("Invalid account directory identifier".into());
        }
        if p.session_directory_id != id {
            return Err("Invalid Session ownership".into());
        }
        Ok(self
            .root
            .join("tdlib/accounts")
            .join(&p.session_directory_id))
    }
    pub fn add(&mut self, name: String) -> Result<String> {
        if name.trim().is_empty() || name.len() > 200 {
            return Err("Invalid account name".into());
        }
        let id = format!(
            "p-{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        if self.registry.profiles.iter().any(|p| p.profile_id == id)
            || self.root.join("tdlib/accounts").join(&id).exists()
        {
            return Err("Account directory already exists; retry creating account".into());
        }
        self.registry.profiles.push(Profile {
            profile_id: id.clone(),
            telegram_user_id: None,
            display_name: name,
            username: None,
            is_legacy_profile: false,
            last_used_at: 0,
            session_directory_id: id.clone(),
            created_at: now(),
        });
        self.save()?;
        Ok(id)
    }
    pub fn rename(&mut self, id: &str, name: String) -> Result<()> {
        if name.trim().is_empty() || name.len() > 200 {
            return Err("Invalid account name".into());
        }
        self.profile(id)?;
        self.registry
            .profiles
            .iter_mut()
            .find(|p| p.profile_id == id)
            .unwrap()
            .display_name = name;
        self.save()
    }
    pub fn select(&mut self, id: &str) -> Result<()> {
        self.profile(id)?;
        self.config.select(Some(id.into()));
        self.save()
    }
    pub fn identify(&mut self, id: &str, user: &serde_json::Value) -> Result<bool> {
        let uid = user["id"]
            .as_i64()
            .ok_or("Invalid Telegram identity")?
            .to_string();
        let duplicate = self
            .registry
            .profiles
            .iter()
            .any(|p| p.profile_id != id && p.telegram_user_id.as_ref() == Some(&uid));
        self.profile(id)?;
        if duplicate {
            return Ok(true);
        }
        let p = self
            .registry
            .profiles
            .iter_mut()
            .find(|p| p.profile_id == id)
            .unwrap();
        if p.telegram_user_id.is_none()
            && (p.display_name == "新账号" || p.display_name == "已有账号 (Legacy)")
        {
            p.display_name = format!(
                "{} {}",
                user["first_name"].as_str().unwrap_or(""),
                user["last_name"].as_str().unwrap_or("")
            )
            .trim()
            .into();
        }
        p.telegram_user_id = Some(uid);
        p.username = user["usernames"]["editable_username"]
            .as_str()
            .map(str::to_owned);
        p.last_used_at = now();
        self.save()?;
        Ok(duplicate)
    }
    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.profile(id)?;
        self.registry.profiles.retain(|p| p.profile_id != id);
        self.credentials.secrets.remove(&format!("db:{id}"));
        if self.config.value.last_profile_id.as_deref() == Some(id) {
            self.config.select(None);
        }
        self.save()
    }
    pub fn delete_session(&self, id: &str) -> Result<()> {
        if self.profile(id)?.is_legacy_profile {
            return Err("Legacy Session 不允许通过账号管理删除；请保留原目录".into());
        }
        let target = self.session_root(id)?;
        if !target.exists() {
            return Ok(());
        }
        // Refuse junctions/symlinks anywhere in the owned directory tree.
        fn inspect(path: &Path) -> Result<()> {
            let metadata =
                fs::symlink_metadata(path).map_err(|_| "Cannot inspect Session directory")?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Session contains a reparse point; deletion refused".into());
                }
            }
            if metadata.file_type().is_symlink() {
                return Err("Session contains a symbolic link; deletion refused".into());
            }
            if metadata.is_dir() {
                for entry in fs::read_dir(path).map_err(|_| "Cannot inspect Session directory")? {
                    inspect(&entry.map_err(|_| "Cannot inspect Session entry")?.path())?;
                }
            }
            Ok(())
        }
        let owner = self.root.join("tdlib/accounts");
        for ancestor in [&self.root, &self.root.join("tdlib"), &owner] {
            let metadata =
                fs::symlink_metadata(ancestor).map_err(|_| "Cannot inspect Session parent")?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("Session parent is a reparse point; deletion refused".into());
                }
            }
            if metadata.file_type().is_symlink() {
                return Err("Session parent is a symbolic link; deletion refused".into());
            }
        }
        let canonical_owner = owner
            .canonicalize()
            .map_err(|_| "Cannot resolve Session owner")?;
        let canonical_target = target
            .canonicalize()
            .map_err(|_| "Cannot resolve Session directory")?;
        if canonical_target.parent() != Some(canonical_owner.as_path())
            || canonical_target.file_name() != Some(std::ffi::OsStr::new(id))
        {
            return Err("Session directory ownership check failed".into());
        }
        inspect(&target)?;
        fs::remove_dir_all(&target).map_err(|_| "Cannot delete local Session data".into())
    }
    pub fn has(&self, key: &str) -> bool {
        self.credentials.secrets.contains_key(key)
    }
    pub fn secret(&self, key: &str) -> Result<Option<String>> {
        self.credentials
            .secrets
            .get(key)
            .map(|v| {
                protect(v, false).and_then(|v| {
                    String::from_utf8(v).map_err(|_| "Saved credential invalid; re-enter it".into())
                })
            })
            .transpose()
    }
    pub fn set_secret(&mut self, key: &str, value: Option<String>) -> Result<()> {
        if key != "api" && key != "proxy" {
            self.profile(key.strip_prefix("db:").ok_or("Invalid credential key")?)?;
        }
        if let Some(v) = value {
            if v.is_empty() {
                return Err("Credential cannot be empty".into());
            }
            self.credentials
                .secrets
                .insert(key.into(), protect(v.as_bytes(), true)?);
        } else {
            self.credentials.secrets.remove(key);
        }
        self.save()
    }
}
#[cfg(windows)]
fn protect(data: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *const u8,
            prompt: *const u8,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *const u8,
            prompt: *const u8,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut u8) -> *mut u8;
    }
    let input = Blob {
        len: u32::try_from(data.len()).map_err(|_| "Credential too large")?,
        data: data.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        len: 0,
        data: std::ptr::null_mut(),
    };
    // Current-user scope, UI forbidden. DPAPI owns output; copy then free it.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(
                "Windows credential protection failed; clear saved credential and re-enter it"
                    .into(),
            );
        }
        let result = std::slice::from_raw_parts(output.data, output.len as usize).to_vec();
        LocalFree(output.data);
        Ok(result)
    }
}
#[cfg(not(windows))]
fn protect(_: &[u8], _: bool) -> Result<Vec<u8>> {
    Err("Credential storage requires Windows DPAPI".into())
}
#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 9) } == 0 {
        Err("Cannot atomically replace settings".into())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).map_err(|_| "Cannot replace settings".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tmsgpicker-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn malformed_or_future_settings_are_never_overwritten() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let file = root.join("settings.json");
        fs::write(&file, b"{broken").unwrap();
        assert!(Store::open(root.clone()).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"{broken");
        let mut value = serde_json::to_value(Disk::default()).unwrap();
        value["settings"]["schema_version"] = serde_json::json!(99);
        fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Store::open(root.clone()).is_err());
        value["settings"]["schema_version"] = serde_json::json!(1);
        value["old_unknown_field"] = serde_json::json!(true);
        fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Store::open(root.clone()).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn identity_duplicate_detection_and_owned_deletion() {
        let root = temp_root();
        fs::create_dir_all(root.join("tdlib/db")).unwrap();
        let mut s = Store::open(root.clone()).unwrap();
        let a = s.add("A".into()).unwrap();
        let b = s.add("B".into()).unwrap();
        let me = serde_json::json!({"id":12345,"first_name":"Synthetic"});
        assert!(!s.identify(&a, &me).unwrap());
        assert!(s.identify(&b, &me).unwrap());
        assert!(s.profile(&b).unwrap().telegram_user_id.is_none());
        assert_eq!(s.profile(&b).unwrap().display_name, "B");
        assert_ne!(s.session_root(&a).unwrap(), s.session_root(&b).unwrap());
        fs::create_dir_all(s.session_root(&a).unwrap().join("db")).unwrap();
        s.delete_session(&a).unwrap();
        assert!(!s.session_root(&a).unwrap().exists());
        assert!(s.delete_session("legacy").is_err());
        assert!(root.join("tdlib/db").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn empty_registry_and_unsaved_key_never_create_or_rebuild_session_directories() {
        let root = temp_root();
        let mut s = Store::open(root.clone()).unwrap();
        assert!(s.profile_ids().is_empty());
        assert!(!root.join("tdlib").exists());
        let id = s.add("Synthetic".into()).unwrap();
        let credentials = s.account_credentials(&id, None).unwrap();
        assert_eq!(credentials.profile_id, id);
        assert!(!credentials.remember_database);
        assert!(credentials.passphrase.is_none());
        assert!(!credentials.session_root.exists());
        let registry: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("accounts.json")).unwrap()).unwrap();
        assert!(registry["profiles"][0].get("api_id").is_none());
        assert!(registry["profiles"][0].get("proxy_enabled").is_none());
        assert!(registry["profiles"][0].get("client_id").is_none());
        assert!(s.profile(&id).unwrap().created_at > 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn v1_migration_preserves_config_identity_ciphertexts_and_original_database() {
        let root = temp_root();
        fs::create_dir_all(root.join("tdlib/db")).unwrap();
        fs::write(root.join("tdlib/db/synthetic.db"), b"original-db").unwrap();
        let mut disk = Disk::default();
        disk.settings.api_id = Some(1234);
        disk.settings.card_compact = true;
        #[cfg(windows)]
        disk.secrets.insert(
            "db:legacy".into(),
            protect(b"synthetic-legacy-passphrase", true).unwrap(),
        );
        let original = serde_json::to_vec(&disk).unwrap();
        fs::write(root.join("settings.json"), &original).unwrap();
        let mut s = Store::open(root.clone()).unwrap();
        assert_eq!(s.profile_ids(), vec!["legacy"]);
        assert_eq!(s.session_root("legacy").unwrap(), root.join("tdlib"));
        assert_eq!(s.settings().api_id, Some(1234));
        assert!(s.settings().card_compact);
        #[cfg(windows)]
        assert_eq!(
            s.account_credentials("legacy", None)
                .unwrap()
                .passphrase
                .as_deref(),
            Some("synthetic-legacy-passphrase")
        );
        assert_eq!(fs::read(root.join("settings.json")).unwrap(), original);
        assert_eq!(
            fs::read(root.join("tdlib/db/synthetic.db")).unwrap(),
            b"original-db"
        );
        s.remove("legacy").unwrap();
        let s = Store::open(root.clone()).unwrap();
        assert!(s.profile_ids().is_empty()); // Removal does not re-import the retained database.
        assert!(root.join("tdlib/db/synthetic.db").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn profile_credentials_and_decryption_failure_are_isolated() {
        let root = temp_root();
        let mut s = Store::open(root.clone()).unwrap();
        let a = s.add("A".into()).unwrap();
        let b = s.add("B".into()).unwrap();
        s.set_secret(&format!("db:{a}"), Some("synthetic-A".into()))
            .unwrap();
        s.set_secret(&format!("db:{b}"), Some("synthetic-B".into()))
            .unwrap();
        assert_eq!(
            s.account_credentials(&a, None)
                .unwrap()
                .passphrase
                .as_deref(),
            Some("synthetic-A")
        );
        assert_eq!(
            s.account_credentials(&b, None)
                .unwrap()
                .passphrase
                .as_deref(),
            Some("synthetic-B")
        );
        s.credentials
            .secrets
            .insert(format!("db:{a}"), vec![1, 2, 3]);
        assert!(s.account_credentials(&a, None).is_err());
        assert_eq!(
            s.account_credentials(&b, None)
                .unwrap()
                .passphrase
                .as_deref(),
            Some("synthetic-B")
        );
        assert_eq!(
            s.account_credentials(&a, Some("replacement".into()))
                .unwrap()
                .passphrase
                .as_deref(),
            Some("replacement")
        );
        s.set_secret(&format!("db:{a}"), None).unwrap();
        assert!(!s.account_credentials(&a, None).unwrap().remember_database);
        assert!(s.account_credentials(&b, None).unwrap().remember_database);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn damaged_secret_requires_manual_replacement_and_clear() {
        let root = temp_root();
        let mut s = Store::open(root.clone()).unwrap();
        s.credentials.secrets.insert("api".into(), vec![1, 2, 3]);
        s.save().unwrap();
        assert!(s.secret("api").is_err());
        s.set_secret("api", Some("synthetic-replacement".into()))
            .unwrap();
        let mut s = Store::open(root.clone()).unwrap();
        assert_eq!(s.secret("api").unwrap().unwrap(), "synthetic-replacement");
        s.set_secret("api", None).unwrap();
        assert!(!s.snapshot().has_saved_api_hash);
        assert!(s.secret("api").unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn compact_preference_defaults_for_existing_configs_and_preserves_accounts() {
        let root = temp_root();
        let mut store = Store::open(root.clone()).unwrap();
        let id = store.add("Synthetic profile".into()).unwrap();
        let mut settings = store.settings();
        settings.left_ratio = 0.27;
        settings.right_ratio = 0.31;
        store.set_settings(settings).unwrap();
        let mut old = serde_json::to_value(Disk {
            settings: store.settings(),
            profiles: store.registry.profiles.clone(),
            legacy_imported: store.registry.legacy_imported,
            secrets: store.credentials.secrets.clone(),
        })
        .unwrap();
        old["settings"]
            .as_object_mut()
            .unwrap()
            .remove("card_compact");
        fs::write(
            root.join("settings.json"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let mut store = Store::open(root.clone()).unwrap();
        assert!(!store.settings().card_compact);
        store.set_card_compact(true).unwrap();
        let store = Store::open(root.clone()).unwrap();
        assert!(store.settings().card_compact);
        assert_eq!(store.settings().left_ratio, 0.27);
        assert_eq!(store.settings().right_ratio, 0.31);
        assert!(store.profile(&id).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn persistence_isolation_legacy_and_secrets() {
        let root = std::env::temp_dir().join(format!(
            "tmsgpicker-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("tdlib/db")).unwrap();
        let mut s = Store::open(root.clone()).unwrap();
        let a = s.add("A".into()).unwrap();
        let b = s.add("B".into()).unwrap();
        assert_ne!(s.session_root(&a).unwrap(), s.session_root(&b).unwrap());
        assert!(s.session_root("../bad").is_err());
        let mut settings = s.settings();
        settings.api_id = Some(123);
        s.set_settings(settings).unwrap();
        #[cfg(windows)]
        {
            s.set_secret("api", Some("synthetic-api-secret".into()))
                .unwrap();
            s.set_secret(&format!("db:{a}"), Some("synthetic-db-secret".into()))
                .unwrap();
            assert_eq!(s.secret("api").unwrap().unwrap(), "synthetic-api-secret");
            let disk = fs::read_to_string(root.join("credential-store.json")).unwrap();
            assert!(!disk.contains("synthetic"));
            assert!(
                !serde_json::to_string(&s.snapshot())
                    .unwrap()
                    .contains("synthetic")
            );
        }
        let mut s = Store::open(root.clone()).unwrap();
        assert_eq!(s.settings().api_id, Some(123));
        assert_eq!(
            s.registry
                .profiles
                .iter()
                .filter(|p| p.is_legacy_profile)
                .count(),
            1
        );
        s.rename(&a, "Alias".into()).unwrap();
        s.reset().unwrap();
        assert!(root.join("tdlib/db").exists());
        assert_eq!(s.profile(&a).unwrap().display_name, "Alias");
        s.remove(&b).unwrap();
        assert!(s.profile(&b).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
