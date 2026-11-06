use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub integration: Integration,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Integration {
    pub backend: Backend,
    pub auto_open: bool,
    pub command: Option<String>,
}

impl Default for Integration {
    fn default() -> Self {
        Self {
            backend: Backend::default(),
            auto_open: true,
            command: None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Herdr,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let directory: PathBuf = env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(Into::into)
            .or_else(|| env::home_dir().map(|home| home.join(".config")))
            .context("Failed to determine configuration directory")?;

        Self::load_from(&directory.join("wt/config.toml"))
    }

    fn load_from(path: &Path) -> anyhow::Result<Self> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read configuration {}", path.display()));
            }
        };

        toml::from_str(&contents)
            .with_context(|| format!("Failed to parse configuration {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_files_and_distinguishes_missing_from_invalid() -> anyhow::Result<()> {
        let timestamp =
            std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH)?;
        let path = env::temp_dir().join(format!(
            "wt-config-test-{}-{}.toml",
            std::process::id(),
            timestamp.as_nanos()
        ));
        assert!(Config::load_from(&path)?.integration.auto_open);
        fs::write(&path, "[integration]\nauto_open = false")?;
        let config = Config::load_from(&path);
        fs::write(&path, "[invalid")?;
        let error = Config::load_from(&path).unwrap_err();
        fs::remove_file(&path)?;

        assert!(!config?.integration.auto_open);
        assert!(error.to_string().contains(&path.display().to_string()));
        assert!(Config::load_from(Path::new(".")).is_err());

        Ok(())
    }

    #[test]
    fn omitted_settings_keep_existing_behavior() {
        for contents in ["", "[integration]", "[integration]\ncommand = 'cargo test'"] {
            let config: Config = toml::from_str(contents).unwrap();
            assert!(matches!(config.integration.backend, Backend::Herdr));
            assert!(config.integration.auto_open);
        }
    }

    #[test]
    fn parses_integration_settings() {
        let config: Config = toml::from_str(
            "[integration]\nbackend = 'herdr'\nauto_open = false\ncommand = 'cargo test'",
        )
        .unwrap();

        assert!(!config.integration.auto_open);
        assert_eq!(config.integration.command.as_deref(), Some("cargo test"));
    }

    #[test]
    fn rejects_invalid_settings() {
        for contents in [
            "[integration]\nbackend = 'unknown'",
            "[integration]\nauto_open = 'false'",
            "[integration]\nauto_opne = false",
            "[integration",
        ] {
            assert!(toml::from_str::<Config>(contents).is_err());
        }
    }
}
