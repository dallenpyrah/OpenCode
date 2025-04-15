use anyhow::{Context, Result};
use keyring::Entry;
use serde::Deserialize;
use std::{env, fs}; // Removed unused PathBuf

const GLOBAL_CONFIG_DIR: &str = "OpenCode";
const GLOBAL_CONFIG_FILE: &str = "config.toml";
const PROJECT_CONFIG_FILE: &str = ".OpenCode.toml";
pub const KEYRING_SERVICE_NAME: &str = "opencode_cli"; // Service name for keyring - Made public
pub const DEFAULT_KEYRING_ENTRY_NAME: &str = "openrouter_api_key"; // Default username/entry name - Made public

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub api: ApiConfig,
    // Add other configuration sections like UI, safety, etc. later
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    /// Reference to the API key stored in the system keyring.
    /// If None, uses the default entry name "openrouter_api_key".
    #[serde(default)]
    pub keyring_entry: Option<String>,

    /// Default OpenRouter model ID to use for requests.
    #[serde(default = "default_model")]
    pub default_model: String,

    // Add other API related settings like base_url, timeout, etc. if needed
}

fn default_model() -> String {
    // A sensible default model
    "anthropic/claude-3.5-sonnet".to_string()
}

impl Config {
    /// Loads configuration from default locations.
    /// Order: Project config (./.OpenCode.toml) overrides Global config (~/.config/OpenCode/config.toml)
    pub fn load() -> Result<Self> {
        let global_config = load_global_config()?;
        let project_config = load_project_config()?;

        // Simple merge: Project overrides global. If neither exists, use default.
        // A more sophisticated field-by-field merge could be implemented if needed.
        match (project_config, global_config) {
            (Some(proj), _) => {
                tracing::info!("Loaded project configuration from .OpenCode.toml");
                Ok(proj)
            }
            (None, Some(glob)) => {
                tracing::info!("Loaded global configuration from ~/.config/OpenCode/config.toml");
                Ok(glob)
            }
            (None, None) => {
                tracing::info!("No configuration file found, using default settings.");
                Ok(Config::default())
            }
        }
    }

    /// Retrieves the API key securely from the system keyring.
    pub fn get_api_key(&self) -> Result<Option<String>> {
        let entry_name = self
            .api
            .keyring_entry
            .as_deref()
            .unwrap_or(DEFAULT_KEYRING_ENTRY_NAME);

        tracing::debug!(
            "Attempting to retrieve API key from keyring service='{}' entry='{}'",
            KEYRING_SERVICE_NAME,
            entry_name
        );

        let entry = Entry::new(KEYRING_SERVICE_NAME, entry_name)?;

        match entry.get_password() {
            Ok(password) => {
                tracing::info!(
                    "Successfully retrieved API key from keyring entry '{}'",
                    entry_name
                );
                Ok(Some(password))
            }
            Err(keyring::Error::NoEntry) => {
                tracing::warn!(
                    "No API key found in keyring for service='{}' entry='{}'. Use the 'configure' command to set it.",
                    KEYRING_SERVICE_NAME, entry_name
                );
                Ok(None)
            }
            Err(e) => {
                tracing::error!("Failed to retrieve API key from keyring: {}", e);
                Err(e).context("Failed to retrieve API key from system keyring")
            }
        }
    }
}

fn load_global_config() -> Result<Option<Config>> {
    match dirs::config_dir() {
        Some(mut path) => {
            path.push(GLOBAL_CONFIG_DIR);
            path.push(GLOBAL_CONFIG_FILE);
            if path.exists() {
                tracing::debug!("Attempting to load global config from: {:?}", path);
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read global config file: {:?}", path))?;
                let config: Config = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse global config file: {:?}", path))?;
                Ok(Some(config))
            } else {
                tracing::debug!("Global config file not found at: {:?}", path);
                Ok(None)
            }
        }
        None => {
            tracing::warn!("Could not determine user config directory.");
            Ok(None)
        }
    }
}

fn load_project_config() -> Result<Option<Config>> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    for ancestor in current_dir.ancestors() {
        let config_path = ancestor.join(PROJECT_CONFIG_FILE);
        if config_path.exists() {
            tracing::debug!("Attempting to load project config from: {:?}", config_path);
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read project config file: {:?}", config_path))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse project config file: {:?}", config_path))?;
            return Ok(Some(config));
        }
    }
    tracing::debug!("No project config file (.OpenCode.toml) found in ancestor directories.");
    Ok(None)
}


// --- Tests ---
// Add tests later according to rules (Task 6.5)