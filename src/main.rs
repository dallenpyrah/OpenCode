use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use keyring::Entry;
use tracing_subscriber::{fmt, EnvFilter};

mod config;
mod api_client;
mod context_manager;
pub mod tui; // Make the tui module public
mod tools;
use api_client::{ApiClient, ChatCompletionRequest, Message, Role}; // Added ChatCompletionRequest
use config::Config;
use context_manager::ContextManager;
use crate::tui::start_spinner; // Import the spinner function
/// A Rust-based CLI AI coding assistant with OpenRouter integration and tool calling.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands, // Changed to required, as configure is now a command
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Configure OpenCode settings, like the API key.
    Configure(ConfigureArgs),
    /// Ask the AI assistant a question.
    Ask { prompt: String },
}

#[derive(Args, Debug)]
struct ConfigureArgs {
    /// Set the OpenRouter API key securely in the system keyring.
    /// Optionally specify a custom entry name for the key.
    #[arg(long, value_name = "KEY_ENTRY_NAME")]
    set_api_key: Option<Option<String>>, // Use Option<Option<T>> for optional value with optional argument
}

// Renamed main to run_app and changed main to handle the result
async fn run_app() -> Result<()> {
    // Initialize tracing subscriber
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    tracing::info!("Application started");

    let cli = Cli::parse();

    // Load configuration first, as it's needed for multiple commands potentially
    let config = Config::load().context("Failed to load configuration")?;
    // Instantiate clients (consider lazy instantiation if needed)
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    let mut context_manager = ContextManager::new(config)?; // Needs to be mutable, unwrap Result

    match cli.command {
        Commands::Configure(args) => {
            if let Some(key_entry_opt) = args.set_api_key {
                let entry_name = key_entry_opt
                    .as_deref()
                    .unwrap_or(config::DEFAULT_KEYRING_ENTRY_NAME);
                set_api_key(entry_name)?;
            } else {
                tui::print_info("Specify an option to configure, e.g., --set-api-key");
            }
        }
        Commands::Ask { prompt } => {
            tracing::debug!("Processing 'ask' command with prompt: '{}'", prompt);

            let user_message = Message {
                role: Role::User,
                content: Some(prompt), // Wrap in Some()
                tool_calls: None, // Add missing field
                tool_call_id: None, // Add missing field
            };

            // Add user message to context
            context_manager.add_message(user_message.clone())?; // Clone needed, handle Result

            // Construct messages for the API call
            let messages_for_api = context_manager.construct_api_messages()?; // Handle Result

            if messages_for_api.is_empty() {
                 anyhow::bail!("Cannot send empty message list to API.");
            }


            let request = ChatCompletionRequest {
                model: context_manager.config().api.default_model.clone(), // Use getter method
                messages: messages_for_api,
                stream: None, // Non-streaming request
                temperature: None,
                // top_p: None, // Field does not exist
                // top_k: None, // Field does not exist
                // frequency_penalty: None, // Field does not exist
                // presence_penalty: None, // Field does not exist
                // seed: None, // Field does not exist
                max_tokens: None,
                // stop: None, // Field does not exist
                tools: None,
                tool_choice: None,
                // response_format: None, // Field does not exist
            };

            tracing::debug!("Sending request to API: {:?}", request);

            let spinner = start_spinner("Waiting for API response..."); // Start spinner

            let result = api_client.chat_completion(request).await; // Store result

            spinner.finish_and_clear(); // Stop spinner regardless of outcome

            match result { // Match on the stored result
                Ok(response) => {
                    tracing::debug!("Received response from API: {:?}", response);
                    if let Some(choice) = response.choices.first() {
                        if let Some(content) = &choice.message.content {
                            tui::print_result(content); // Use TUI for result output

                            // Optional: Add assistant response back to context
                            let assistant_message = Message {
                                role: Role::Assistant,
                                content: Some(content.clone()), // Wrap in Some()
                                tool_calls: None, // Add missing field
                                tool_call_id: None, // Add missing field
                            };
                            context_manager.add_message(assistant_message)?; // Handle Result
                            tracing::debug!("Added assistant response to context.");

                        } else {
                            tui::print_warning("Assistant response content was empty."); // Use TUI for warning
                            tracing::warn!("Assistant response content was None.");
                        }
                    } else {
                        tui::print_warning("No choices received from API."); // Use TUI for warning
                        tracing::warn!("No choices received in API response.");
                    }
                }
                Err(e) => {
                    // Print a user-friendly error message
                    // Use TUI for error output
                    tui::print_error(&format!("Error interacting with the AI: {}", e));
                    // Optionally, return the error to stop execution if desired
                    // return Err(e.context("API call failed"));
                }
            }
        }
    }

    tracing::info!("Application finished");
    Ok(())
}


#[tokio::main]
async fn main() {
    // Execute the main application logic
    if let Err(e) = run_app().await {
        // Use TUI to print the error, including its causes
        tui::print_error(&format!("Application failed: {:?}", e));
        std::process::exit(1); // Exit with a non-zero status code
    }
}

/// Prompts user for API key and stores it in the keyring.
fn set_api_key(entry_name: &str) -> Result<()> {
    tui::print_info(
        "Please enter your OpenRouter API key (it will not be displayed):"
    ); // Use TUI for info
    let api_key = rpassword::prompt_password("API Key: ")
        .context("Failed to read API key from prompt")?;

    if api_key.trim().is_empty() {
        anyhow::bail!("API key cannot be empty.");
    }

    tracing::debug!(
        "Attempting to store API key in keyring service='{}' entry='{}'",
        config::KEYRING_SERVICE_NAME,
        entry_name
    );

    let entry = Entry::new(config::KEYRING_SERVICE_NAME, entry_name)?;
    entry
        .set_password(&api_key)
        .context("Failed to store API key in system keyring")?;

    tui::print_info(&format!( // Use TUI for info, format needed
        "API key successfully stored in keyring entry '{}'.",
        entry_name
    ));
    tracing::info!(
        "Successfully stored API key in keyring entry '{}'",
        entry_name
    );

    Ok(())
}
