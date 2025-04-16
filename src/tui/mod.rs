use anyhow::Context;
use iocraft::prelude::*;
use std::io::stdout;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use dialoguer::Confirm;
use similar::{ChangeTag, TextDiff};
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};

pub fn print_info(message: &str) {
    element! { Text(content: format!("{}\n", message)) }.print();
}

pub fn print_warning(message: &str) {
    element! {
        Text(color: Color::Yellow, content: format!("Warning: {}\n", message))
    }
    .print();
}

pub fn print_error(message: &str) {
    element! {
        Text(color: Color::Red, content: format!("Error: {}\n", message))
    }
    .print();
}

pub fn print_result(content: &str) {
    element! { Text(content: format!("{}\n", content)) }.print();
}

#[allow(dead_code)]
pub fn print_diff(old_text: &str, new_text: &str) -> anyhow::Result<()> {
    let diff = TextDiff::from_lines(old_text, new_text);
    let mut stdout = stdout();

    for change in diff.iter_all_changes() {
        let (prefix, color) = match change.tag() {
            ChangeTag::Delete => ("-", crossterm::style::Color::Red),
            ChangeTag::Insert => ("+", crossterm::style::Color::Green),
            ChangeTag::Equal => (" ", crossterm::style::Color::Reset),
        };

        crossterm::execute!(stdout, crossterm::style::SetForegroundColor(color))
            .context("Failed to set foreground color")?;

        for line in change.value().lines() {
            crossterm::execute!(stdout, crossterm::style::Print(prefix), crossterm::style::Print(line), crossterm::style::Print("\n"))
                .context("Failed to print diff line")?;
        }

        crossterm::execute!(stdout, crossterm::style::ResetColor).context("Failed to reset color")?;
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

#[derive(Props, Clone, Default)]
pub struct StreamingOutputProps {
    pub stream_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<Result<String, String>>>>>,
}

#[component]
pub fn StreamingOutput(mut hooks: Hooks, props: &StreamingOutputProps) -> impl Into<AnyElement<'static>> {
    let mut content = hooks.use_state(String::new);
    let mut error_message = hooks.use_state(|| None::<String>);
    let rx_ref = props.stream_rx.clone();

    hooks.use_future(async move {
        let mut stream_rx = {
            let mut guard = rx_ref.lock().unwrap();
            guard.take()
        };
        if let Some(mut rx) = stream_rx {
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(chunk) => {
                        let current = content.to_string();
                        let new_content = format!("{}{}", current, chunk);
                        content.set(new_content);
                    },
                    Err(e) => {
                        error_message.set(Some(format!("\nError during streaming: {}", e)));
                        break;
                    }
                }
            }
        }
    });

    element! {
        View() {
            Text(content: content.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}