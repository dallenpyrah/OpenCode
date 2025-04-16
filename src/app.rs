use anyhow::{Context, Result}; // Keep Context and Result
use clap::Parser;
// Removed std::fs
use std::fs;
use std::path::{Path, PathBuf};
use serde_json::json;
// Removed tokio::sync::mpsc import
use tracing_subscriber::{fmt, EnvFilter};

use crate::api::client::ApiClient;
use crate::cli::commands::{Cli, Commands}; // Removed ShellCommands
use crate::config::Config;
use crate::context::ContextManager;
use crate::tools::execution::{SecurityPolicy, ToolExecutionEngine};
use crate::tools::registry::ToolRegistry;
// Removed TUI imports

// Import command handlers (assuming they exist in submodules)
use crate::commands::{
    configure::handle_configure,
    ask::handle_ask,
    generate::handle_generate,
    explain::handle_explain,
    edit::handle_edit,
    debug::handle_debug,
    test_cmd::handle_test,
    doc::handle_doc,
    run::handle_run,
    shell::handle_shell,
};
use crate::interactive::run_interactive_mode;


pub fn generate_source_map(dir: &Path) -> Result<String> {
    let map = json!({});
    let mut stack: Vec<(PathBuf, serde_json::Value)> = vec![(dir.to_path_buf(), map.clone())];

    while let Some((current_path, mut current_level_val)) = stack.pop() {
        if !current_path.is_dir() {
            continue;
        }

        let current_level = current_level_val.as_object_mut().ok_or_else(|| anyhow::anyhow!("Internal error: Expected JSON object"))?;

        for entry in fs::read_dir(&current_path)? {
            let entry = entry?;
            let path = entry.path();
            let file_name_os = path.file_name().ok_or_else(|| anyhow::anyhow!("Could not get file name"))?;
            let file_name = file_name_os.to_str().ok_or_else(|| anyhow::anyhow!("Filename is not valid UTF-8"))?;

            // Skip common unnecessary directories/files
            if file_name == ".git" || file_name == "target" || file_name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                let dir_entry = current_level.entry(file_name.to_string()).or_insert(json!({}));
                stack.push((path, dir_entry.clone())); // Clone the value to push onto stack
            } else if path.is_file() {
                // Consider adding checks for file extensions or types if needed
                current_level.insert(file_name.to_string(), json!(null));
            }
        }
         // Assign the modified level back if necessary (though maybe not needed with direct mutation?)
         // If we pop 'map' initially, this assignment happens back to it implicitly via mutation.
    }

    // Return the initial map which has been mutated
    serde_json::to_string(&map).context("Failed to serialize source map to JSON")
}

pub async fn run() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::builder().parse("info").unwrap())
        .init();

    tracing::info!("Application started");

    // Reverted: Removed terminal initialization and TUI app setup

    // Reverted: Command handling logic runs directly, not in a separate task
    let cli = Cli::parse();
    let config = Config::load().context("Failed to load configuration")?;
    let context_manager = ContextManager::new(config.clone())?;
    let tool_registry = ToolRegistry::new(&config);
    let tool_engine = ToolExecutionEngine::new(&tool_registry, SecurityPolicy::ConfirmWrites);

    let command_result = if let Some(command) = cli.command {
        match command {
            Commands::Configure(args) => {
                handle_configure(config, args).await
            }
            Commands::Ask { prompt } => {
                handle_ask(config, context_manager, &tool_registry, &tool_engine, prompt).await
            }
            Commands::Generate(args) => {
                handle_generate(config, args).await
            }
            Commands::Explain(args) => {
                handle_explain(config, args).await
            }
            Commands::Edit(args) => {
                handle_edit(config, &tool_registry, &tool_engine, args).await
            }
            Commands::Debug(args) => {
                handle_debug(config, args).await
            }
            Commands::Test(args) => {
                handle_test(config, args).await
            }
            Commands::Doc(args) => {
                handle_doc(config, args).await
            }
            Commands::Run(args) => {
                handle_run(config, context_manager, &tool_registry, &tool_engine, args).await
            }
            Commands::Shell(shell_args) => {
                handle_shell(config, shell_args).await
            }
        }
    } else {
        tracing::info!("No subcommand provided, entering interactive mode.");
        let api_client = ApiClient::new(config.clone())
            .context("Failed to create API client for interactive mode (check API key configuration)")?;
        run_interactive_mode(config, api_client, context_manager, &tool_registry, &tool_engine).await
    };

    // Reverted: Removed TUI run loop and terminal restoration logic

    tracing::info!("Application finished");

    // Return the command result directly
    command_result.context("Command execution failed")
}