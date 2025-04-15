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




pub fn print_info(message: &str) {
    let mut stdout = stdout();
    
    
    let _ = execute!(stdout, Print(message), Print("\n")); 
}


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


pub fn print_result(content: &str) {
    let mut stdout = stdout();
    
    
    let _ = execute!(stdout, Print(content), Print("\n")); 
}


#[allow(dead_code)]
pub fn print_code(code: &str, language_hint: Option<&str>) -> anyhow::Result<()> {
    let ps = SyntaxSet::load_defaults_nonewlines();
    let ts = ThemeSet::load_defaults();

    let syntax = language_hint
        .and_then(|hint| ps.find_syntax_by_token(hint)) 
        .or_else(|| ps.find_syntax_by_first_line(code)) 
        .unwrap_or_else(|| ps.find_syntax_plain_text()); 

    
    let theme = &ts.themes["base16-ocean.dark"]; 
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut stdout = stdout();

    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(line, &ps)
            .with_context(|| format!("Failed to highlight line: {}", line))?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        
        
        execute!(stdout, Print(&escaped), ResetColor)
            .with_context(|| format!("Failed to write highlighted line to stdout: {}", line))?;
        
        
    }
    
    execute!(stdout, ResetColor).context("Failed to reset terminal color")?;

    Ok(())
}

use similar::{ChangeTag, TextDiff};




#[allow(dead_code)]
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

        
        for line in change.value().lines() {
            execute!(stdout, Print(prefix), Print(line), Print("\n"))
                .context("Failed to print diff line")?;
        }

        execute!(stdout, ResetColor).context("Failed to reset color")?;
    }

    Ok(())
}





pub fn start_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.blue} {msg}")
            .unwrap()
            
            
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





#[allow(dead_code)]
pub fn prompt_confirmation(prompt_message: &str) -> anyhow::Result<bool> {
    Confirm::new()
        .with_prompt(prompt_message)
        .default(false) 
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
        
        
        let result = print_diff(old, new);
        assert!(result.is_ok(), "print_diff failed: {:?}", result.err());

        let result_no_change = print_diff("same", "same");
        assert!(result_no_change.is_ok(), "print_diff failed on no change: {:?}", result_no_change.err());

        let result_all_change = print_diff("old", "new");
        assert!(result_all_change.is_ok(), "print_diff failed on all change: {:?}", result_all_change.err());
    }


    #[test]
    fn test_prompt_confirmation_compiles() {
        
        
        
        
        let _func: fn(&str) -> anyhow::Result<bool> = prompt_confirmation;
    }