use std::io::Write;
use std::process::ExitCode;

use ghostd::config::DEFAULT_TOML;

const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("default-config"), None) => {
            // Piping into `head` closes stdout early, which must not panic like `print!` does.
            match std::io::stdout().write_all(DEFAULT_TOML.as_bytes()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::FAILURE,
            }
        }
        _ => {
            eprintln!("usage: ghostd default-config");
            ExitCode::from(EXIT_USAGE)
        }
    }
}
