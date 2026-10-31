//! `re-konung-viewer` — просмотрщик карт «Князя».
//!
//! Без подкоманды открывает окно; `check` работает headless и возвращает
//! ненулевой код, если хотя бы одна карта не собралась.

use std::process::ExitCode;

use clap::Parser;
use kn_viewer::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Check { map, all, format }) => kn_viewer::run_check(&cli, map, all, format),
        None => kn_viewer::run_viewer(cli),
    }
}
