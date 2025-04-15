use anyhow::{Context, Result}; // Removed anyhow
use keyring::Entry;

use crate::config::{Config, DEFAULT_KEYRING_ENTRY_NAME, KEYRING_SERVICE_NAME};
use crate::cli::commands::ConfigureArgs;
use crate::tui::{print_info};

pub async fn handle_configure(config: Config, args: ConfigureArgs) -> Result<()> {
    let mut config_to_save = config.clone();
    let mut config_updated = false;

    if let Some(ref key_entry_opt) = args.set_api_key {
        let entry_name = key_entry_opt
            .as_deref()
            .unwrap_or(DEFAULT_KEYRING_ENTRY_NAME);
        set_api_key(entry_name)?;
    }

    if let Some(model_id) = args.set_default_model {
        if model_id.trim().is_empty() {
            anyhow::bail!("Default model ID cannot be empty.");
        }
        config_to_save.api.default_model = model_id;
        config_updated = true;
        print_info(&format!("Default model set to: {}", config_to_save.api.default_model));
    }

    if let Some(model_id) = args.set_edit_model {
         if model_id.trim().is_empty() {
            anyhow::bail!("Edit model ID cannot be empty.");
        }
        config_to_save.api.edit_model = model_id;
        config_updated = true;
        print_info(&format!("Edit model set to: {}", config_to_save.api.edit_model));
    }

    if config_updated {
        config_to_save.save().context("Failed to save updated configuration")?;
        print_info("Configuration saved successfully.");
    } else if args.set_api_key.is_none() {
         print_info("Specify an option to configure, e.g., --set-api-key, --set-default-model, --set-edit-model");
    }
    Ok(())
}

fn set_api_key(entry_name: &str) -> Result<()> {
    print_info(
        "Please enter your OpenRouter API key (it will not be displayed):"
    );
    let api_key = rpassword::prompt_password("API Key: ")
        .context("Failed to read API key from prompt")?;

    if api_key.trim().is_empty() {
        anyhow::bail!("API key cannot be empty.");
    }

    tracing::debug!(
        "Attempting to store API key in keyring service='{}' entry='{}'",
        KEYRING_SERVICE_NAME,
        entry_name
    );

    let entry = Entry::new(KEYRING_SERVICE_NAME, entry_name)?;
    entry
        .set_password(&api_key)
        .context("Failed to store API key in system keyring")?;

    print_info(&format!(
        "API key successfully stored in keyring entry '{}'.",
        entry_name
    ));
    tracing::info!(
        "Successfully stored API key in keyring entry '{}'",
        entry_name
    );

    Ok(())
}