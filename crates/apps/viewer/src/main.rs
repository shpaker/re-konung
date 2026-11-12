//! `re-konung-viewer` — the «Князь» map viewer.
//!
//! Without a subcommand it opens the window (kn-fe-viewer); `check` runs headless
//! and returns a nonzero code if at least one map failed to build.

use std::process::ExitCode;

use clap::Parser;
use kn_fe_viewer::AppExit;
use kn_viewer::cli::{Cli, Command};

/// The app's exit as this process's exit code: the code a benchmark run's
/// failed gate carries (`Error(1)`) must survive `cargo run` and `just
/// viewer-gate`, not drown in an unconditional success.
fn exit_code_of(exit: AppExit) -> ExitCode {
    match exit {
        AppExit::Success => ExitCode::SUCCESS,
        AppExit::Error(code) => ExitCode::from(code.get()),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Check { map, all, format }) => kn_viewer::run_check(&cli, map, all, format),
        Some(Command::Simulate {
            map,
            all,
            ticks,
            seed,
            format,
        }) => kn_viewer::run_simulate(&cli, map, all, ticks, seed, format),
        None => match kn_viewer::viewer_config(&cli) {
            Ok(config) => exit_code_of(kn_fe_viewer::run_viewer(config)),
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use super::*;

    #[test]
    fn the_app_exit_reaches_the_process_exit_code() {
        assert_eq!(exit_code_of(AppExit::Success), ExitCode::SUCCESS);
        let error = AppExit::Error(NonZero::new(1).unwrap());
        assert_eq!(exit_code_of(error), ExitCode::from(1));
    }
}
