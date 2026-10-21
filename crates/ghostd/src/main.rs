use std::io::Write;
use std::process::ExitCode;

use ghostd::collect::{GpuCollector, GpuSource, NvmlSource, Source};
use ghostd::config::DEFAULT_TOML;

const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["default-config"] => print_stdout(DEFAULT_TOML),
        ["probe", "gpu"] => probe_gpu(),
        _ => {
            eprintln!("usage: ghostd <default-config | probe gpu>");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

fn print_stdout(text: &str) -> ExitCode {
    // Piping into `head` closes stdout early, which must not panic like `print!` does.
    match std::io::stdout().write_all(text.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Prints one GPU sample as a line of JSON. A machine without a readable GPU is not a failure: it
/// prints `present:false`, which is exactly what the daemon would publish.
fn probe_gpu() -> ExitCode {
    let source = NvmlSource::open().map(|source| Box::new(source) as Box<dyn GpuSource>);
    let snapshot = GpuCollector::new(source).sample();
    match serde_json::to_string(&snapshot) {
        Ok(json) => print_stdout(&format!("{json}\n")),
        Err(error) => {
            eprintln!("ghostd: cannot encode the gpu snapshot: {error}");
            ExitCode::FAILURE
        }
    }
}
