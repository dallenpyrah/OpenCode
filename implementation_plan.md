# OpenCode Implementation Tasks Document

This document outlines the tasks required to implement the OpenCode CLI tool based on the provided `design.md`.

## Phase 1: Project Setup & Core Infrastructure

*   **Task 1.1: Initialize Rust Project:**
    *   Create a new Rust binary project using `cargo new opencode --bin`.
    *   Set up the basic project structure (src, tests, etc.).
    *   Initialize Git repository.
*   **Task 1.2: Add Core Dependencies:**
    *   Add essential crates to `Cargo.toml`: `tokio`, `clap`, `serde`, `serde_json`, `toml`, `reqwest`, `thiserror`, `anyhow`, `tracing`, `tracing-subscriber`, `crossterm`, `keyring`.
*   **Task 1.3: Implement Basic CLI Parsing:**
    *   Define initial command structure using `clap` (e.g., main entry point, `--help`, `--version`, basic subcommands placeholder).
    *   Implement argument parsing logic in `main.rs`.
*   **Task 1.4: Setup Logging/Tracing:**
    *   Configure `tracing` and `tracing-subscriber` for structured logging (output to stderr, configurable levels via flags/env vars).
*   **Task 1.5: Implement Configuration Management:**
    *   Define configuration structures (using `serde`) for API key, default model, etc.
    *   Implement loading from global (`~/.config/OpenCode/config.toml`) and project (`./.OpenCode.toml`) TOML files.
    *   Implement secure API key storage/retrieval using the `keyring` crate.
    *   Create a `configure` subcommand to set the API key.
*   **Task 1.6: Basic Error Handling:**
    *   Set up custom error types using `thiserror`.
    *   Use `anyhow` for application-level error wrapping and reporting.
    *   Implement graceful error display to the user via stderr.

## Phase 2: OpenRouter API Integration & Context Management

*   **Task 2.1: Implement OpenRouter API Client:**
    *   Create an API client module.
    *   Implement functions for:
        *   Making authenticated requests (using `reqwest` or `openrouter_api` crate).
        *   Sending chat completion requests (standard, streaming, tool call).
        *   Handling API responses (parsing JSON, SSE streams).
        *   Handling API errors (network, rate limits, provider errors).
*   **Task 2.2: Implement Context Manager:**
    *   Create a module for managing conversation history and context.
    *   Implement token counting (e.g., using `tiktoken-rs` or estimation).
    *   Implement logic for adding explicit context (files, lines, potentially URLs).
    *   Implement basic context window management (e.g., simple eviction).
    *   Integrate context gathering into the API client calls.
*   **Task 2.3: Basic AI Interaction Command:**
    *   Implement a simple command (e.g., `opencode ask "prompt"`) that:
        *   Takes a user prompt.
        *   Constructs a basic API request using the Context Manager and API Client.
        *   Sends the request to OpenRouter.
        *   Displays the LLM response to the user via the TUI Renderer.

## Phase 3: Terminal UI (TUI) Rendering

*   **Task 3.1: Implement Basic Output Formatting:**
    *   Create a TUI Renderer module.
    *   Use `crossterm` for basic text output, colors, and attributes.
    *   Implement functions for displaying informational messages, errors, and results.
*   **Task 3.2: Implement Syntax Highlighting:**
    *   Integrate `syntect` for code block rendering.
    *   Include necessary syntax definitions (`syntect-assets`).
*   **Task 3.3: Implement Diff Viewing:**
    *   Integrate `similar` or `diffy` crate.
    *   Implement rendering of colored diffs (unified format).
*   **Task 3.4: Implement Progress Indicators:**
    *   Add spinners or progress bars (e.g., using `indicatif` or manual `crossterm`) for long-running operations like API calls.
*   **Task 3.5: Implement User Prompts/Confirmation:**
    *   Use `crossterm` or `dialoguer`/`inquire` for handling interactive yes/no confirmations.

## Phase 4: Tool Calling Engine & Built-in Tools (Expanded Detail)

*   **Task 4.1: Define Core Tool Abstraction:**
    *   **Task 4.1.1:** Define the `CliTool` trait with methods:
        *   `name() -> String`
        *   `description() -> String`
        *   `parameters_schema() -> serde_json::Value` (returns JSON Schema for inputs)
        *   `execute(args: serde_json::Value) -> Result<serde_json::Value, ToolError>` (takes parsed args, returns JSON result or specific ToolError)
    *   **Task 4.1.2:** Define `ToolError` enum for specific tool execution failures (e.g., `FileNotFound`, `PermissionDenied`, `CommandFailed`, `InvalidArguments`, `NetworkError`). Ensure it implements `std::error::Error` and `Display`.
*   **Task 4.2: Implement Tool Registry and Management:**
    *   **Task 4.2.1:** Create a `ToolRegistry` struct to hold registered `Box<dyn CliTool + Send + Sync>`.
    *   **Task 4.2.2:** Implement methods in `ToolRegistry` to register built-in tools during initialization.
    *   **Task 4.2.3:** Implement `ToolRegistry::get_tool_schemas()` method to generate the list of tool schemas (`Vec<serde_json::Value>`) required for the OpenRouter API call (`tools` array).
    *   **Task 4.2.4:** Implement `ToolRegistry::get_tool(name: &str)` method to look up a tool implementation by name.
*   **Task 4.3: Implement Tool Call Parsing and Validation:**
    *   **Task 4.3.1:** Implement logic (likely within the API Client or Core Logic) to detect `tool_calls` (OpenAI style) or `stop_reason: tool_use` (Anthropic style) in LLM responses.
    *   **Task 4.3.2:** Extract tool name(s), arguments (as `serde_json::Value`), and unique call ID(s) (`tool_call_id` / `tool_use_id`) from the response structure.
    *   **Task 4.3.3:** Integrate a JSON Schema validator crate (e.g., `jsonschema`).
    *   **Task 4.3.4:** For each requested tool call, retrieve the tool's schema from the `ToolRegistry` and validate the received arguments against it. Map validation errors to `ToolError::InvalidArguments`.
*   **Task 4.4: Implement Tool Execution Engine and Security:**
    *   **Task 4.4.1:** Create the main `ToolExecutionEngine` module/struct, likely taking the `ToolRegistry` and `TuiRenderer` as dependencies.
    *   **Task 4.4.2:** Implement the core `ToolExecutionEngine::execute_tool_call(call_request)` method. This method orchestrates lookup, validation (if not done earlier), security checks, execution, and result formatting for a single tool call request.
    *   **Task 4.4.3:** Define security levels/policies enum (e.g., `ReadOnly`, `ConfirmWrites`, `ConfirmAll`, `Disabled`). Load the active policy from the `ConfigurationManager`.
    *   **Task 4.4.4:** Implement a security check function `needs_confirmation(tool_name: &str, policy: SecurityPolicy) -> bool`. This function determines if a specific tool requires confirmation based on its known risk profile and the active policy.
    *   **Task 4.4.5:** If confirmation is needed, call the TUI Renderer's confirmation prompt (Task 3.5), clearly displaying the tool name, arguments, and potential impact. Handle user 'yes'/'no' response. Return a specific `ToolError` (e.g., `ConfirmationDenied`) if denied.
    *   **Task 4.4.6:** Call the appropriate `CliTool::execute` method with the validated arguments.
    *   **Task 4.4.7:** Implement robust error handling for `ToolError` returned by `execute`, logging details and preparing the error for reporting back to the LLM.
*   **Task 4.5: Implement Built-in Tool Logic:**
    *   **Task 4.5.1:** Implement `FileReadTool` struct satisfying `CliTool`. Handles file reading logic and errors. Security: `ReadOnly`.
    *   **Task 4.5.2:** Implement `FileWriteTool` struct satisfying `CliTool`. Handles file writing logic and errors. Security: Requires confirmation unless policy is `Disabled`.
    *   **Task 4.5.3:** Implement `ShellCommandTool` struct satisfying `CliTool`. Uses `std::process::Command` or `tokio::process::Command`. Captures stdout, stderr, exit code. Security: Requires confirmation unless policy is `Disabled`. Implement careful argument handling/escaping if applicable.
    *   **Task 4.5.4:** Implement `GitTool` struct satisfying `CliTool`. Define arguments for sub-operations (e.g., `{"operation": "status"}`, `{"operation": "commit", "message": "..."}`). Use `git2` crate or shell out to `git` CLI. Security: Mark state-changing operations (`commit`, `add`, `push` etc.) as requiring confirmation.
    *   **Task 4.5.5:** Implement `WebSearchTool` struct satisfying `CliTool`. Decide on implementation (e.g., wrapper around a search API like SerpAPI, requires API key config) or potentially basic HTTP GET for direct URL fetching. Handle network errors. Security: Generally `ReadOnly`, but depends on implementation.
    *   **Task 4.5.6:** Implement `CodeSearchTool` struct satisfying `CliTool`. Use `ripgrep` crate or shell out to `rg` command. Security: `ReadOnly`.
*   **Task 4.6: Implement Tool Result Formatting:**
    *   **Task 4.6.1:** Implement a function `format_tool_result(call_id: String, tool_name: String, result: Result<serde_json::Value, ToolError>) -> serde_json::Value`.
    *   **Task 4.6.2:** Based on the `Result`, construct the appropriate JSON structure expected by the OpenRouter API for either a successful tool result or an error result, ensuring the correct `tool_call_id` / `tool_use_id` is included. Distinguish between OpenAI and Anthropic formats if necessary based on the ongoing conversation or configuration.
*   **Task 4.7: Integrate Tool Cycle into API Flow:**
    *   **Task 4.7.1:** Modify the API Client / Core Logic to fetch tool schemas from the `ToolRegistry` (Task 4.2.3) and include them in the `tools` array of the outgoing OpenRouter API request.
    *   **Task 4.7.2:** Ensure the main interaction loop, upon receiving a response with tool calls, iterates through each call request and invokes `ToolExecutionEngine::execute_tool_call` (Task 4.4.2).
    *   **Task 4.7.3:** Collect the formatted results (Task 4.6.1) for all executed tool calls.
    *   **Task 4.7.4:** Append these formatted tool results as new messages (role `tool` or content type `tool_result`) to the conversation history before sending the next request back to the OpenRouter API to get the final response.

## Phase 5: Core AI Features & Agentic Workflow

*   **Task 5.1: Implement Code Generation Command:**
    *   Create command (e.g., `opencode generate "description" --file context.rs`).
    *   Pass prompt and context to LLM.
    *   Display generated code (with syntax highlighting).
*   **Task 5.2: Implement Code Explanation Command:**
    *   Create command (e.g., `opencode explain --file code.rs --lines 10-20`).
    *   Send code snippet/file to LLM.
    *   Display explanation.
*   **Task 5.3: Implement Code Editing/Refactoring (via Tools):**
    *   Design prompts for editing/refactoring tasks that guide the LLM to use `file_read` and `file_write` tools.
    *   Create a command (e.g., `opencode edit "instruction" --file code.rs`).
*   **Task 5.4: Implement Debugging Assistance:**
    *   Create command (e.g., `opencode debug --error "message" --file code.rs`).
    *   Send error and code context to LLM.
    *   Display suggestions.
*   **Task 5.5: Implement Test Generation:**
    *   Create command (e.g., `opencode test --file code.rs`).
    *   Send code to LLM, prompt for test generation.
    *   Display generated tests.
*   **Task 5.6: Implement Documentation Generation:**
    *   Create command (e.g., `opencode doc --file code.rs`).
    *   Send code to LLM, prompt for docstrings/comments.
    *   Display generated documentation.
*   **Task 5.7: Implement Basic Agentic Task Execution:**
    *   Design the core loop for multi-step tasks.
    *   Implement logic to parse LLM plans (sequences of tool calls).
    *   Execute plan steps via the Tool Engine, handling confirmations and results.
    *   Provide feedback to the user during execution.
*   **Task 5.8: Implement Shell Command Assistance:**
    *   Create `explain` subcommand (e.g., `opencode shell explain "command"`).
    *   Create `suggest` subcommand (e.g., `opencode shell suggest "description"`).

## Phase 6: Advanced Features & Polish

*   **Task 6.1: Implement Interactive Mode (REPL):**
    *   (Optional) Integrate `clap-repl` or build a custom REPL.
    *   Maintain session state (history, context) within the REPL.
    *   Implement context management commands (`/context add`, `/list`, `/clear`, etc.).
*   **Task 6.2: Implement User-Defined Tools:**
    *   Define configuration format for custom tools (name, description, schema, execution command/script).
    *   Load and integrate custom tools into the Tool Engine.
*   **Task 6.3: Advanced Context Management:**
    *   Implement `--symbol` context flag using `tree-sitter`.
    *   Implement context summarization strategies.
*   **Task 6.4: Refine TUI/UX:**
    *   Improve output formatting based on user feedback.
    *   (Optional) Explore `ratatui` for richer interactive views.
*   **Task 6.5: Add Comprehensive Tests:**
    *   Write unit tests for core logic, parsers, context manager, tool implementations.
    *   Write integration tests for CLI commands and basic API interactions (potentially using mock servers or limited live calls).
*   **Task 6.6: Write Documentation:**
    *   Create README.md with usage instructions, installation guide, configuration details.
    *   Document command-line options (`--help` output).
    *   Document tool usage and security implications.

## Phase 7: Release & Future Considerations

*   **Task 7.1: Packaging & Distribution:**
    *   Set up CI/CD for building releases (e.g., GitHub Actions).
    *   Consider packaging for common platforms (e.g., Homebrew, Cargo install).
*   **Task 7.2: Address Potential Challenges:**
    *   Review mitigations for context limits, security, UX, latency, cost, compatibility (as outlined in `design.md`).
*   **Task 7.3: Plan Future Enhancements:**
    *   Prioritize items from the "Future Directions" section of `design.md`.

## High-Level Phase Diagram

```mermaid
graph TD
    A[Phase 1: Setup & Core Infra] --> B[Phase 2: API & Context];
    B --> C[Phase 3: TUI Rendering];
    C --> D[Phase 4: Tool Calling Engine & Tools];
        subgraph Phase 4 Tasks
            D1[4.1 Define Tool Trait] --> D2[4.2 Tool Registry];
            D2 --> D3[4.3 Parse/Validate Calls];
            D3 --> D4[4.4 Execution Engine & Security];
            D4 --> D5[4.5 Implement Built-in Tools];
            D5 --> D6[4.6 Format Tool Results];
            D6 --> D7[4.7 Integrate API Flow];
        end
    D --> E[Phase 5: Core AI Features & Agent];
    E --> F[Phase 6: Advanced Features & Polish];
    F --> G[Phase 7: Release & Future];