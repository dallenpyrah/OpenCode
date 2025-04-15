use crossterm::{
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use std::io::{stderr, stdout};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use dialoguer::Confirm;

use anyhow::Context;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::{as_24_bit_terminal_escaped, LinesWithEndings},
};

// Using standalone functions for simplicity as no state is needed yet.

/// Prints an informational message to stdout.
pub fn print_info(message: &str) {
    let mut stdout = stdout();
    // Optional: Add a prefix or style if desired, e.g., bold
    // let _ = execute!(stdout, Print(style(message).bold()), Print("\n"), ResetColor);
    let _ = execute!(stdout, Print(message), Print("\n")); // Simple print for now
}

/// Prints a warning message to stderr in yellow.
pub fn print_warning(message: &str) {
    let mut stderr = stderr();
    let _ = execute!(
        stderr,
        SetForegroundColor(Color::Yellow),
        Print("Warning: "),
        Print(message),
        Print("\n"),
        ResetColor
    );
}

/// Prints an error message to stderr in red.
pub fn print_error(message: &str) {
    let mut stderr = stderr();
    let _ = execute!(
        stderr,
        SetForegroundColor(Color::Red),
        Print("Error: "),
        Print(message),
        Print("\n"),
        ResetColor
    );
}

/// Prints the main result content to stdout.
pub fn print_result(content: &str) {
    let mut stdout = stdout();
    // Optional: Add visual distinction, e.g., slight indent or prefix
    // let _ = execute!(stdout, Print("  "), Print(content), Print("\n"), ResetColor);
    let _ = execute!(stdout, Print(content), Print("\n")); // Simple print for now
}

/// Prints code with syntax highlighting.
pub fn print_code(code: &str, language_hint: Option<&str>) -> anyhow::Result<()> {
    let ps = SyntaxSet::load_defaults_nonewlines();
    let ts = ThemeSet::load_defaults();

    let syntax = language_hint
        .and_then(|hint| ps.find_syntax_by_token(hint)) // Try hint first
        .or_else(|| ps.find_syntax_by_first_line(code)) // Then try first line
        .unwrap_or_else(|| ps.find_syntax_plain_text()); // Fallback to plain text

    // Using a common theme, ensure it exists or handle potential panic/error
    let theme = &ts.themes["base16-ocean.dark"]; 
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut stdout = stdout();

    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(line, &ps)
            .with_context(|| format!("Failed to highlight line: {}", line))?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        
        // Print the line with highlighting and ensure reset at the end
        execute!(stdout, Print(&escaped), ResetColor)
            .with_context(|| format!("Failed to write highlighted line to stdout: {}", line))?;
        // Note: LinesWithEndings includes the newline, so no extra Print("\n") needed unless the last line is missing one.
        // If the last line might not have a newline, add: if !line.ends_with('\n') { execute!(stdout, Print("\n"))?; }
    }
    // Ensure final reset just in case
    execute!(stdout, ResetColor).context("Failed to reset terminal color")?;

    Ok(())
}

use similar::{ChangeTag, TextDiff};

/// Prints a colored diff of two text blocks.
///
/// Uses the `similar` crate to compute the diff and `crossterm` for coloring.
/// Lines prefixed with '+' are additions (green).
/// Lines prefixed with '-' are deletions (red).
/// Unchanged lines are printed normally.
pub fn print_diff(old_text: &str, new_text: &str) -> anyhow::Result<()> {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut stdout = stdout();

    for change in diff.iter_all_changes() {
        let (prefix, color) = match change.tag() {
            ChangeTag::Delete => ("-", Color::Red),
            ChangeTag::Insert => ("+", Color::Green),
            ChangeTag::Equal => (" ", Color::Reset),
        };

        execute!(stdout, SetForegroundColor(color))
            .context("Failed to set foreground color")?;

        // Print prefix and each line of the change
        for line in change.value().lines() {
            execute!(stdout, Print(prefix), Print(line), Print("\n"))
                .context("Failed to print diff line")?;
        }

        execute!(stdout, ResetColor).context("Failed to reset color")?;
    }

    Ok(())
}


/// Creates and starts a spinner with the given message.
///
/// The caller is responsible for calling `.finish()` or similar on the returned ProgressBar.
/// The spinner is drawn to stderr.
pub fn start_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.blue} {msg}")
            .unwrap()
            // For more spinners check out the cli-spinners project:
            // https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
            .tick_strings(&[
                "⠋",
                "⠙",
                "⠹",
                "⠸",
                "⠼",
                "⠴",
                "⠦",
                "⠧",
                "⠇",
                "⠏",
            ]),
    );
    pb.set_message(message.to_string());
    pb
}


/// Prompts the user for a yes/no confirmation.
///
/// Uses the `dialoguer` crate to display the prompt.
/// Returns `true` if the user confirms (yes), `false` otherwise (no/default).
pub fn prompt_confirmation(prompt_message: &str) -> anyhow::Result<bool> {
    Confirm::new()
        .with_prompt(prompt_message)
        .default(false) // Default to No if the user just presses Enter
        .interact()
        .context("Failed to get user confirmation")
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_code_rust() {
        let rust_code = r#"
fn main() {
    let x = 5;
    println!("Hello, world! {}", x);
}
"#;
        let result = print_code(rust_code, Some("rust"));
        // Basic check: Ensure it runs without panicking or returning an error.
        // Verifying terminal output is complex and platform-dependent.
        assert!(result.is_ok(), "print_code failed: {:?}", result.err());
    }

    #[test]
    fn test_print_code_no_hint() {
        let python_code = "def greet(name):\n    print(f'Hello, {name}!')\n";
        let result = print_code(python_code, None);
        assert!(result.is_ok(), "print_code failed: {:?}", result.err());
    }

     #[test]
    fn test_print_code_plain_text() {
        let text = "This is just plain text.\nNo syntax here.";
        let result = print_code(text, Some("unknown-language"));
        assert!(result.is_ok(), "print_code failed: {:?}", result.err());
    }
}

    #[test]
    fn test_print_diff() {
        let old = "Hello\nWorld\nSame";
        let new = "Hello\nRust\nSame";
        // This test primarily checks if the function runs without panicking.
        // Visual verification of the output is needed manually.
        let result = print_diff(old, new);
        assert!(result.is_ok(), "print_diff failed: {:?}", result.err());

        let result_no_change = print_diff("same", "same");
        assert!(result_no_change.is_ok(), "print_diff failed on no change: {:?}", result_no_change.err());

        let result_all_change = print_diff("old", "new");
        assert!(result_all_change.is_ok(), "print_diff failed on all change: {:?}", result_all_change.err());
    }


    #[test]
    fn test_prompt_confirmation_compiles() {
        // This test mainly ensures the function signature is correct and compiles.
        // Actual interaction testing is complex and often done manually or with
        // more sophisticated test setups (e.g., mocking stdin/stdout or specific
        // library features if available).
        let _func: fn(&str) -> anyhow::Result<bool> = prompt_confirmation;
    }

