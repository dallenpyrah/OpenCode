use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

pub const GLOBAL_CONFIG_DIR: &str = "OpenCode";
const GLOBAL_CONFIG_FILE: &str = "config.toml";
const PROJECT_CONFIG_FILE: &str = ".OpenCode.toml";
pub const KEYRING_SERVICE_NAME: &str = "opencode_cli"; 
pub const DEFAULT_KEYRING_ENTRY_NAME: &str = "openrouter_api_key"; 

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct UserToolConfig {
    pub name: String,
    pub description: String,
    pub input_schema: String, 
    pub command_template: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub api: ApiConfig,
    

    #[serde(default)]
    pub usertools: Option<Vec<UserToolConfig>>,

    #[serde(skip)]
    brave_search_api_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)] 
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    
    
    #[serde(default)]
    pub keyring_entry: Option<String>,

    
    #[serde(default = "default_model")]
    pub default_model: String,

    
    #[serde(default = "default_edit_model")]
    pub edit_model: String,

    
    #[serde(default = "default_big_model")]
    pub big_model: String,
}

fn default_model() -> String {
    "google/gemini-2.5-pro-preview-03-25".to_string()
}

fn default_edit_model() -> String {
    "google/gemini-2.0-flash-001".to_string()
}

fn default_big_model() -> String {
    "google/gemini-2.5-pro-preview-03-25".to_string()
}


impl Default for ApiConfig {
    fn default() -> Self {
        ApiConfig {
            keyring_entry: None, 
            default_model: default_model(),
            edit_model: default_edit_model(),
            big_model: default_big_model(),
        }
    }
}
impl Config {
    
    
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();
        let global_config = load_global_config()?;
        let project_config = load_project_config()?;

        
        
        let mut config = match (project_config, global_config) {
            (Some(proj), _) => {
                tracing::info!("Loaded project configuration from .OpenCode.toml");
                proj
            }
            (None, Some(glob)) => {
                tracing::info!("Loaded global configuration from ~/.config/OpenCode/config.toml");
                glob
            }
            (None, None) => {
                tracing::info!("No configuration file found, using default settings.");
                Config::default()
            }
        };

        
        if let Ok(big_model) = env::var("OPENCODE_BIG_MODEL") {
            if !big_model.is_empty() {
                tracing::info!("Using big model from OPENCODE_BIG_MODEL environment variable: {}", big_model);
                config.api.big_model = big_model;
            } else {
                tracing::warn!("OPENCODE_BIG_MODEL environment variable is set but empty.");
            }
        } else {
            tracing::debug!("OPENCODE_BIG_MODEL environment variable not set, using config/default value: {}", config.api.big_model);
        }

        if let Ok(edit_model) = env::var("OPENCODE_EDIT_MODEL") {
            if !edit_model.is_empty() {
                tracing::info!("Using edit model from OPENCODE_EDIT_MODEL environment variable: {}", edit_model);
                config.api.edit_model = edit_model;
            } else {
                tracing::warn!("OPENCODE_EDIT_MODEL environment variable is set but empty.");
            }
        } else {
            tracing::debug!("OPENCODE_EDIT_MODEL environment variable not set, using config/default value: {}", config.api.edit_model);
        }

        match env::var("BRAVE_SEARCH_API_KEY") {
            Ok(key) if !key.is_empty() => {
                tracing::info!("Using Brave Search API key from BRAVE_SEARCH_API_KEY environment variable.");
                config.brave_search_api_key = Some(key);
            }
            Ok(_) => {
                tracing::warn!("BRAVE_SEARCH_API_KEY environment variable is set but empty.");
            }
            Err(env::VarError::NotPresent) => {
                tracing::debug!("BRAVE_SEARCH_API_KEY environment variable not found.");
            }
            Err(e) => {
                tracing::error!("Error reading BRAVE_SEARCH_API_KEY environment variable: {}", e);
            }
        }
Ok(config)
}

// Removed unused brave_search_api_key method


    pub fn get_api_key(&self) -> Result<Option<String>> {
        
        match env::var("OPENROUTER_API_KEY") {
            Ok(key) if !key.is_empty() => {
                tracing::info!("Using API key from OPENROUTER_API_KEY environment variable.");
                return Ok(Some(key));
            }
            Ok(_) => {
                tracing::warn!("OPENROUTER_API_KEY environment variable is set but empty.");
                
            }
            Err(env::VarError::NotPresent) => {
                
                tracing::debug!("OPENROUTER_API_KEY environment variable not found.");
            }
            Err(e) => {
                
                tracing::error!("Error reading OPENROUTER_API_KEY environment variable: {}", e);
                
            }
        }

        
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

    
    
    pub fn save(&self) -> Result<()> {
        let config_path = find_project_config_path()?.unwrap_or_else(|| {
            
            let current_dir = env::current_dir().expect("Failed to get current directory");
            current_dir.join(PROJECT_CONFIG_FILE)
        });

        tracing::info!("Saving configuration to: {:?}", config_path);
        let toml_string = toml::to_string_pretty(self)
            .context("Failed to serialize configuration to TOML")?;

        fs::write(&config_path, toml_string)
            .with_context(|| format!("Failed to write configuration file: {:?}", config_path))?;

        Ok(())
    }
}



fn find_project_config_path() -> Result<Option<PathBuf>> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
    for ancestor in current_dir.ancestors() {
        let config_path = ancestor.join(PROJECT_CONFIG_FILE);
        if config_path.exists() {
            return Ok(Some(config_path));
        }
    }
    Ok(None)
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
    if let Some(config_path) = find_project_config_path()? {
        tracing::debug!("Attempting to load project config from: {:?}", config_path);
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read project config file: {:?}", config_path))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse project config file: {:?}", config_path))?;
        Ok(Some(config))
    } else {
        tracing::debug!("No project config file (.OpenCode.toml) found in ancestor directories.");
        Ok(None)
    }
}