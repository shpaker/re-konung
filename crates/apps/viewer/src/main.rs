//! `re-konung-viewer` — the «Князь» map viewer.
//!
//! Without a subcommand it opens the window (kn-fe-viewer); `check` runs headless
//! and returns a nonzero code if at least one map failed to build.

use std::process::ExitCode;

use clap::Parser;
use kn_viewer::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Check { map, all, format }) => kn_viewer::run_check(&cli, map, all, format),
        None => match kn_viewer::viewer_config(&cli) {
            Ok(config) => {
                kn_fe_viewer::run_viewer(config);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
    }
}
