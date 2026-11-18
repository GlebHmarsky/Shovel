use super::Settings;
use url::Url;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum AvatarProvider {
    #[default]
    Repository,
    Gravatar,
    Initials,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AvatarPreferences {
    pub provider: AvatarProvider,
    pub site: String,
}
pub fn avatar_website(input: &str) -> Option<Url> {
    if input.len() > 2048 {
        return None;
    }
    let url = Url::parse(&format!("{}/", input.trim().trim_end_matches('/'))).ok()?;
    (matches!(url.scheme(), "https" | "http")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}
impl AvatarPreferences {
    pub fn from_settings(settings: &Settings) -> Self {
        let value = settings.get("Avatars");
        let provider = match value.and_then(|v| v.get("Provider")).map(|v| v.as_u64()) {
            None | Some(Some(0)) => AvatarProvider::Repository,
            Some(Some(1)) => AvatarProvider::Gravatar,
            _ => AvatarProvider::Initials,
        };
        Self {
            provider,
            site: value
                .and_then(|v| v.get("GiteaServer"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
        }
    }
    pub fn valid(&self) -> bool {
        self.site.trim().is_empty() || avatar_website(&self.site).is_some()
    }
    pub fn save(&self, settings: &mut Settings) {
        let mut value = settings
            .get("Avatars")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        value.insert("Provider".into(), (self.provider as u8).into());
        value.insert("GiteaServer".into(), self.site.as_str().into());
        settings.set("Avatars", value.into());
    }
    pub(super) fn normalize(settings: &mut Settings) {
        let mut prefs = Self::from_settings(settings);
        prefs.site = if prefs.valid() {
            prefs.site.trim().trim_end_matches('/').to_owned()
        } else {
            String::new()
        };
        prefs.save(settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsStore;
    #[test]
    fn avatar_preferences_import_normalize_and_roundtrip_without_dropping_extra_fields() {
        let profile = tempfile::tempdir().unwrap();
        let mut store = SettingsStore::new(profile.path().into());
        let mut settings = store.load().unwrap();
        assert_eq!(
            AvatarPreferences::from_settings(&settings).provider,
            AvatarProvider::Repository
        );
        settings.set("Avatars",serde_json::json!({"Provider":1,"GiteaServer":" https://git.example.test/projects/ ","Extra":"preserve"}));
        settings.normalize();
        store.save(&settings).unwrap();
        let mut loaded = SettingsStore::new(profile.path().into()).load().unwrap();
        let actual = AvatarPreferences::from_settings(&loaded);
        assert_eq!(
            actual,
            AvatarPreferences {
                provider: AvatarProvider::Gravatar,
                site: "https://git.example.test/projects".into()
            }
        );
        assert_eq!(loaded.get("Avatars").unwrap()["Extra"], "preserve");
        loaded.set("Avatars",serde_json::json!({"Provider":-1,"GiteaServer":"https://user:example@git.example.test/"}));
        loaded.normalize();
        let invalid = AvatarPreferences::from_settings(&loaded);
        assert_eq!(invalid.provider, AvatarProvider::Initials);
        assert!(invalid.site.is_empty());
    }
}
