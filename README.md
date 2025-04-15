# OpenCode

A command-line tool written in Rust designed to assist developers with various coding tasks, potentially leveraging large language models via services like Open Router.

## Features (Inferred from codebase structure)

*   **Ask (`ask`):** Ask questions about specific code sections.
*   **Explain (`explain`):** Get explanations for code snippets or files.
*   **Generate (`generate`):** Generate code based on prompts.
*   **Document (`doc`):** Assist with writing code documentation.
*   **Debug (`debug`):** Help with debugging code.
*   **Run (`run`):** Execute code or scripts.
*   **Shell (`shell`):** Interact with the shell in the context of the project.
*   **Test (`test_cmd`):** Run tests or test-related commands.
*   **Configure (`configure`):** Configure the tool, potentially setting up API keys or other settings.
*   **Interactive Mode:** Offers an interactive session for commands.
*   **Code Parsing:** Includes capabilities for parsing source code.

## Installation

(Assuming standard Rust setup)

1.  Ensure you have Rust and Cargo installed.
2.  Clone the repository.
3.  Build the project: `cargo build --release`
4.  The executable can be found at `./target/release/opencode` (or similar).

Alternatively, install directly using:
`cargo install --path .`

## Basic Usage

(Hypothetical - actual commands might differ)

```bash
opencode <command> [arguments...]

# Example: Explain a function in a file
opencode explain src/app.rs --function my_function

# Example: Ask a question about a file
opencode ask "What does this struct do?" src/parsing/code_parser.rs

# Example: Configure the tool
opencode configure
```

*(Note: This documentation is auto-generated based on file structure and may require updates based on actual implementation.)*
