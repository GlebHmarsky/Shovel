//! Compatible settings with independent Rust storage. Unknown baseline fields
//! are retained so importing settings cannot silently discard existing choices.
use crate::updates::{Channel, PRODUCT};
use serde_json::{Map, Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

const MAX_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Settings {
    values: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut result = Self { values: Map::new() };
        result.normalize();
        result
    }
}

impl Settings {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }
    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.values.insert(name.into(), value);
    }
    pub fn channel(&self) -> Channel {
        if self.get("UpdateChannel").and_then(Value::as_str) == Some("win-rust-stable") {
            Channel::Stable
        } else {
            Channel::Beta
        }
    }
    pub fn set_channel(&mut self, channel: Channel) {
        self.set("UpdateChannel", json!(channel.id()));
    }
    pub fn ui_scale(&self) -> f64 {
        self.get("UiScale").and_then(Value::as_f64).unwrap_or(1.0)
    }
    pub fn repositories(&self) -> Vec<PathBuf> {
        self.get("RepositoryPaths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(PathBuf::from)
            .collect()
    }

    pub fn normalize(&mut self) {
        self.set("SchemaVersion", json!(6));
        self.set("RustProfileVersion", json!(1));
        self.set("Product", json!(PRODUCT));
        let channel = self.channel();
        self.set_channel(channel);
        for (name, fallback, min, max) in [
            ("UiScale", 1., 0.7, 2.),
            ("DiffFontSize", 12., 9., 24.),
            ("WindowWidth", 1200., 900., 3840.),
            ("WindowHeight", 800., 620., 2160.),
            ("SidebarWidth", 210., 160., f64::MAX),
            ("DetailsHeight", 180., 80., 3000.),
            ("HistoryFilesWidth", 340., 160., 3000.),
        ] {
            let mut value = self
                .get(name)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .unwrap_or(fallback)
                .clamp(min, max);
            if name == "UiScale" {
                value = (value * 10.).round() / 10.;
            }
            self.set(name, json!(value));
        }
        for (name, fallback) in [
            ("HistoryRefsOnRight", false),
            ("DiffSideBySide", false),
            ("CheckUpdatesPeriodically", true),
            ("AutoFetchEnabled", true),
            ("IsMaximized", false),
        ] {
            let value = self.get(name).and_then(Value::as_bool).unwrap_or(fallback);
            self.set(name, json!(value));
        }
        let revision = self
            .get("AutoFetchCadenceRevision")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let seconds = self
            .get("AutoFetchIntervalSeconds")
            .and_then(Value::as_i64)
            .unwrap_or(60);
        self.set(
            "AutoFetchIntervalSeconds",
            json!(if revision == 0 && seconds == 10 {
                60
            } else {
                seconds.clamp(10, 3600)
            }),
        );
        self.set("AutoFetchCadenceRevision", json!(1));
        let appearance = self
            .get("Appearance")
            .and_then(Value::as_u64)
            .filter(|n| *n <= 1)
            .unwrap_or(0);
        self.set("Appearance", json!(appearance));
        let mut paths = Vec::<String>::new();
        for path in self
            .get("RepositoryPaths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if paths.len() == 20 {
                break;
            }
            if Path::new(path).is_absolute()
                && !path.contains('\0')
                && !paths
                    .iter()
                    .any(|p| p.to_lowercase() == path.to_lowercase())
            {
                paths.push(path.to_owned());
            }
        }
        self.set("RepositoryPaths", json!(paths));
        for (name, max) in [
            ("NetworkActions", 60),
            ("WindowLayouts", 32),
            ("CollapsedSidebarGroups", 256),
            ("RepositoryIdentities", 20),
            ("RepositoryPreferences", 20),
        ] {
            let items = self
                .get(name)
                .and_then(Value::as_array)
                .map(|v| v.iter().take(max).cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            self.set(name, json!(items));
        }
    }

    fn decode(bytes: &[u8], importing: bool) -> Result<Self, SettingsError> {
        let value: Value = serde_json::from_slice(bytes).map_err(|_| SettingsError::Invalid)?;
        let mut values = value.as_object().cloned().ok_or(SettingsError::Invalid)?;
        let schema = values
            .get("SchemaVersion")
            .and_then(Value::as_u64)
            .ok_or(SettingsError::Invalid)?;
        if schema > 6 {
            return Err(SettingsError::NewerSchema);
        }
        if schema == 0 {
            return Err(SettingsError::Invalid);
        }
        if !importing
            && (values.get("Product").and_then(Value::as_str) != Some(PRODUCT)
                || values.get("RustProfileVersion").and_then(Value::as_u64) != Some(1))
        {
            return Err(SettingsError::Invalid);
        }
        if schema == 3
            && let Some(legacy) = values.get("Approvals").cloned()
        {
            values.insert("RepositoryIdentities".into(), legacy);
        }
        if schema < 3 {
            values.insert("RepositoryIdentities".into(), json!([]));
            values.insert("RepositoryPreferences".into(), json!([]));
        }
        if importing {
            // A C# channel string is never persisted into the Rust profile.
            let channel =
                if values.get("UpdateChannel").and_then(Value::as_str) == Some("win-stable") {
                    Channel::Stable
                } else {
                    Channel::Beta
                };
            values.insert("UpdateChannel".into(), json!(channel.id()));
        }
        let mut settings = Self { values };
        settings.normalize();
        Ok(settings)
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("settings are invalid or unsupported; the original file was preserved")]
    Invalid,
    #[error("settings were written by a newer application; the original file was preserved")]
    NewerSchema,
    #[error("settings changed in another process; current bytes were preserved")]
    Changed,
    #[error("settings exceed the supported size")]
    TooLarge,
    #[error("settings cannot be saved until a valid profile has been loaded")]
    ReadOnly,
    #[error("settings could not be read or written")]
    Io(#[from] std::io::Error),
}

pub struct SettingsStore {
    directory: PathBuf,
    expected: Option<Vec<u8>>,
    writable: bool,
}

impl SettingsStore {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            expected: None,
            writable: false,
        }
    }
    pub fn path(&self) -> PathBuf {
        self.directory.join("settings.json")
    }
    pub fn load(&mut self) -> Result<Settings, SettingsError> {
        self.writable = false;
        self.expected = read_bounded(&self.path())?;
        let settings = match self.expected.as_deref() {
            Some(bytes) => Settings::decode(bytes, false)?,
            None => Settings::default(),
        };
        self.writable = true;
        Ok(settings)
    }
    /// Import is a read of C# settings and a later ordinary save to this store.
    /// Existing Rust settings always take precedence. Drafts are migrated separately.
    pub fn load_or_import(
        &mut self,
        csharp_settings: &Path,
    ) -> Result<(Settings, bool), SettingsError> {
        let current = self.load()?;
        if self.expected.is_some() {
            return Ok((current, false));
        }
        match read_bounded(csharp_settings)? {
            None => Ok((current, false)),
            Some(bytes) => Ok((Settings::decode(&bytes, true)?, true)),
        }
    }
    pub fn save(&mut self, settings: &Settings) -> Result<(), SettingsError> {
        if !self.writable {
            return Err(SettingsError::ReadOnly);
        }
        let mut normalized = settings.clone();
        normalized.normalize();
        let bytes =
            serde_json::to_vec_pretty(&normalized.values).map_err(|_| SettingsError::Invalid)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(SettingsError::TooLarge);
        }
        fs::create_dir_all(&self.directory)?;
        // Serialize cooperating writers. A failed lock never overwrites data.
        let mut lock_options = OpenOptions::new();
        lock_options
            .read(true)
            .write(true)
            .create(true)
            .truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            lock_options.share_mode(0);
        }
        let _lock = lock_options.open(self.directory.join("settings-writer.lock"))?;
        if read_bounded(&self.path())? != self.expected {
            return Err(SettingsError::Changed);
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        if read_bounded(&self.path())? != self.expected {
            return Err(SettingsError::Changed);
        }
        if self.expected.is_none() {
            temporary
                .persist_noclobber(self.path())
                .map_err(|e| SettingsError::Io(e.error))?;
        } else {
            // Close the temporary file handle before ReplaceFileW; its open
            // sharing flags need not grant Windows rename/delete permission.
            let temporary = temporary.into_temp_path();
            replace_with_backup(&temporary, &self.path())?;
        }
        self.expected = Some(bytes);
        Ok(())
    }
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, SettingsError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if file.metadata()?.len() > MAX_BYTES {
        return Err(SettingsError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(SettingsError::TooLarge);
    }
    Ok(Some(bytes))
}

#[cfg(windows)]
fn replace_with_backup(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<u16>>()
    };
    let source = wide(source);
    let backup = wide(&destination.with_extension("json.bak"));
    let destination = wide(destination);
    // SAFETY: each immutable UTF-16 input is NUL-terminated and lives through
    // the call; no optional exclusion or reserved data pointers are supplied.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_with_backup(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::copy(destination, destination.with_extension("json.bak"))?;
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_all_fields_without_modifying_csharp_and_roundtrips() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("csharp.json");
        let original=br#"{"SchemaVersion":6,"UpdateChannel":"win-beta","UiScale":1.3,"AutoFetchIntervalSeconds":10,"ExternalMergeTool":{"Mode":2,"CustomCommand":"tool $MERGED"},"UnknownFuturePreference":"retained"}"#;
        fs::write(&source, original).unwrap();
        let root = temp.path().join("rust");
        let mut store = SettingsStore::new(root.clone());
        let (settings, imported) = store.load_or_import(&source).unwrap();
        assert!(imported);
        assert_eq!(settings.channel(), Channel::Beta);
        assert_eq!(settings.ui_scale(), 1.3);
        assert_eq!(settings.get("AutoFetchIntervalSeconds"), Some(&json!(60)));
        store.save(&settings).unwrap();
        assert_eq!(fs::read(source).unwrap(), original);
        let actual = SettingsStore::new(root).load().unwrap();
        assert_eq!(
            actual.get("ExternalMergeTool"),
            settings.get("ExternalMergeTool")
        );
        assert_eq!(
            actual.get("UnknownFuturePreference"),
            Some(&json!("retained"))
        );
    }
    #[test]
    fn preserves_bad_or_newer_profiles_and_refuses_saves() {
        for bytes in [
            &b"broken"[..],
            &br#"{"SchemaVersion":7}"#[..],
            &br#"{"SchemaVersion":6,"Product":"Shovel"}"#[..],
        ] {
            let temp = tempfile::tempdir().unwrap();
            let mut store = SettingsStore::new(temp.path().into());
            fs::write(store.path(), bytes).unwrap();
            assert!(store.load().is_err());
            assert!(matches!(
                store.save(&Settings::default()),
                Err(SettingsError::ReadOnly)
            ));
            assert_eq!(fs::read(store.path()).unwrap(), bytes);
        }
    }
    #[test]
    fn competing_writer_preserved_and_successful_save_keeps_backup() {
        let temp = tempfile::tempdir().unwrap();
        let mut first = SettingsStore::new(temp.path().into());
        let mut settings = first.load().unwrap();
        first.save(&settings).unwrap();
        let original = fs::read(first.path()).unwrap();
        let mut second = SettingsStore::new(temp.path().into());
        let other = second.load().unwrap();
        settings.set("UiScale", json!(1.4));
        first.save(&settings).unwrap();
        assert_eq!(
            fs::read(first.path().with_extension("json.bak")).unwrap(),
            original
        );
        assert!(matches!(second.save(&other), Err(SettingsError::Changed)));
        assert_eq!(
            SettingsStore::new(temp.path().into())
                .load()
                .unwrap()
                .ui_scale(),
            1.4
        );
    }
    #[test]
    fn explicit_fetch_interval_survives_and_channels_cannot_be_csharp() {
        let mut settings = Settings::default();
        settings.set("AutoFetchIntervalSeconds", json!(10));
        settings.normalize();
        assert_eq!(settings.get("AutoFetchIntervalSeconds"), Some(&json!(10)));
        settings.set("UpdateChannel", json!("win-stable"));
        settings.normalize();
        assert_eq!(settings.channel(), Channel::Beta);
        settings.set_channel(Channel::Stable);
        settings.normalize();
        assert_eq!(settings.channel(), Channel::Stable);
    }
}
