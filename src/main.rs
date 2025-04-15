use crate::tui::print_error;

mod app;
mod commands;
mod interactive;
mod streaming;

mod api;
mod cli;
mod config;
mod context;
mod parsing;
mod tools;
mod tui;

#[tokio::main]
async fn main() {
    if let Err(e) = app::run().await {
        print_error(&format!("Application failed: {:?}", e));
        std::process::exit(1);
    }
}