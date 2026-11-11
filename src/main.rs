use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "Usage: cobfmt [--check] [FILE]\n\
Format free-format COBOL from FILE or stdin and write it to stdout.\n\
With --check, exit 0 if formatted, 1 for changes, or 2 on error.";

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut check = false;
    let mut path = None;
    let mut positional = false;
    for arg in env::args_os().skip(1) {
        if !positional && arg == "--" {
            positional = true;
        } else if !positional && arg == "--check" {
            check = true;
        } else if !positional && (arg == "-h" || arg == "--help") {
            println!("{HELP}");
            return Ok(ExitCode::SUCCESS);
        } else if !positional && arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option {}\n{HELP}", arg.to_string_lossy()).into());
        } else if path.replace(PathBuf::from(arg)).is_some() {
            return Err(format!("expected a single input file\n{HELP}").into());
        }
    }
    let input = match path {
        Some(path) => fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        None => {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input)?;
            input
        }
    };
    let formatted = cobfmt::format_source(&input)?;
    if check {
        return Ok(ExitCode::from(u8::from(input != formatted)));
    }
    io::stdout().lock().write_all(formatted.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    run().unwrap_or_else(|error| {
        eprintln!("cobfmt: {error}");
        ExitCode::from(2)
    })
}
