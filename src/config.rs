use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hotkey_spec::HotkeySpec;
use crate::language::LanguageId;
use crate::provider::ProviderKind;

pub const CONFIG_FILE: &str = "config.toml";
pub const SECRETS_FILE: &str = "secrets.bin";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub version: u32,
    pub provider: ProviderKind,
    pub azure_region: String,
    pub fallback_to_azure: bool,
    pub hotkey: String,
    pub language1: String,
    pub language2: String,
    pub autostart: bool,
    pub deepl_usage_known: bool,
    pub deepl_character_count: u64,
    pub deepl_character_limit: u64,
    pub azure_usage_month: String,
    pub azure_usage_chars: u64,
    pub azure_connected: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            provider: ProviderKind::Deepl,
            azure_region: "japaneast".to_string(),
            fallback_to_azure: false,
            hotkey: "ctrl+alt+t".to_string(),
            language1: "ko".to_string(),
            language2: "ja".to_string(),
            autostart: false,
            deepl_usage_known: false,
            deepl_character_count: 0,
            deepl_character_limit: 1_000_000,
            azure_usage_month: String::new(),
            azure_usage_chars: 0,
            azure_connected: false,
        }
    }
}

impl AppConfig {
    pub fn normalize(mut self) -> Self {
        if self.version == 0 {
            self.version = 1;
        }
        if LanguageId::from_id(&self.language1).is_none() {
            self.language1 = "ko".to_string();
        }
        if LanguageId::from_id(&self.language2).is_none() {
            self.language2 = "ja".to_string();
        }
        if HotkeySpec::parse(&self.hotkey).is_none() {
            self.hotkey = "ctrl+alt+t".to_string();
        }
        if self.azure_region.trim().is_empty() {
            self.azure_region = "japaneast".to_string();
        }
        if self.deepl_character_limit == 0 {
            self.deepl_character_limit = 1_000_000;
        }
        self
    }

    pub fn language1(&self) -> LanguageId {
        LanguageId::from_id(&self.language1).unwrap_or(LanguageId::Ko)
    }

    pub fn language2(&self) -> LanguageId {
        LanguageId::from_id(&self.language2).unwrap_or(LanguageId::Ja)
    }

    pub fn hotkey_spec(&self) -> HotkeySpec {
        HotkeySpec::parse(&self.hotkey).unwrap_or_else(|| HotkeySpec::parse("ctrl+alt+t").unwrap())
    }
}

pub fn load_config(path: &Path) -> Result<AppConfig, String> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let config: AppConfig = toml::from_str(&text).map_err(|err| err.to_string())?;
    Ok(config.normalize())
}

pub fn save_config(path: &Path, config: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let body = toml::to_string_pretty(config).map_err(|err| err.to_string())?;
    let text =
        format!("# clip-translator settings. API keys are encrypted in secrets.bin.\n{body}");
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|err| err.to_string())?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|err| err.to_string())
}

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE)
}

pub fn secrets_path(dir: &Path) -> PathBuf {
    dir.join(SECRETS_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_product() {
        let config = AppConfig::default();
        assert_eq!(config.provider, ProviderKind::Deepl);
        assert_eq!(config.language1, "ko");
        assert_eq!(config.language2, "ja");
        assert_eq!(config.hotkey, "ctrl+alt+t");
        assert_eq!(config.azure_region, "japaneast");
        assert!(!config.fallback_to_azure);
        assert!(!config.autostart);
        assert_eq!(config.deepl_character_limit, 1_000_000);
    }

    #[test]
    fn roundtrip_keeps_settings_and_never_writes_a_key() {
        let dir =
            std::env::temp_dir().join(format!("clip-translator-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = config_path(&dir);
        let mut config = AppConfig::default();
        config.fallback_to_azure = true;
        config.hotkey = "ctrl+shift+f5".to_string();
        config.deepl_usage_known = true;
        config.deepl_character_count = 132_580;
        save_config(&path, &config).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("deepl_key"));
        assert!(!text.contains("azure_key"));
        assert!(text.contains("encrypted in secrets.bin"));
        assert!(text.contains("fallback_to_azure = true"));
        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded.hotkey, config.hotkey);
        assert_eq!(loaded.deepl_character_count, 132_580);
        assert!(loaded.fallback_to_azure);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_values_fall_back() {
        let config = AppConfig {
            language1: "nope".into(),
            language2: String::new(),
            hotkey: "not a hotkey".into(),
            azure_region: "  ".into(),
            deepl_character_limit: 0,
            version: 0,
            ..AppConfig::default()
        }
        .normalize();
        assert_eq!(config.language1, "ko");
        assert_eq!(config.language2, "ja");
        assert_eq!(config.hotkey, "ctrl+alt+t");
        assert_eq!(config.azure_region, "japaneast");
        assert_eq!(config.deepl_character_limit, 1_000_000);
        assert_eq!(config.version, 1);
    }

    #[test]
    fn broken_toml_is_an_error_and_missing_file_is_the_default() {
        let dir = std::env::temp_dir().join(format!(
            "clip-translator-config-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("nope.toml");
        assert_eq!(load_config(&missing).unwrap(), AppConfig::default());
        let broken = dir.join("config.toml");
        fs::write(&broken, "provider = [\n").unwrap();
        assert!(load_config(&broken).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
