mod rom_format;
mod search;

use clap::{Parser, ValueEnum};
use rom_format::RomFormat;
use search::guess_file;
use std::{
    io::ErrorKind,
    path::PathBuf,
    process::{Command, ExitCode},
};
use walkdir::WalkDir;

/// RomComp - a ROM compressor that picks the best compression options for you and supports as many ROM formats as possible

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// location of ROM(s) to process.
    /// If its a file, only this file will be processed.
    /// If its a folder, all ROMs inside that folder will be processed
    location: PathBuf,

    /// the rom format that should be compressed

    #[arg(value_enum)]
    format: SourceRomFormat,

    /// enable additional debug messages

    #[arg(short, long, action)]
    debug: bool,
}

#[derive(ValueEnum, Clone, Eq, PartialEq, Debug)]
enum SourceRomFormat {
    Psx,
    Ps2,
    Psp,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if !cli.location.exists() {
        println!("The path {} doesn't exist.", cli.location.to_str().unwrap());
        return ExitCode::from(1);
    }

    let fmt = match cli.format {
        SourceRomFormat::Psx => RomFormat::PSX,
        SourceRomFormat::Ps2 => RomFormat::PS2,
        SourceRomFormat::Psp => RomFormat::PSP,
    };

    if cli.format == SourceRomFormat::Psx || cli.format == SourceRomFormat::Ps2 {
        match Command::new("chdman").spawn() {
            Err(e) => {
                if let ErrorKind::NotFound = e.kind() {
                    println!("You'll need to have CHDMAN available on your PATH if you want to convert these ROMs. Please run this application from Docker or install CHDMAN manually and try again.");
                    return ExitCode::from(2);
                }
            }
            _ => (),
        }
    }

    if cli.location.is_file()
        && !guess_file(&cli.location)
            .map(|f| f.contains(fmt))
            .unwrap_or(false)
    {
        println!(
            "The input file isn't recognized as proper file format for a {:?} rom",
            cli.format
        );
        return ExitCode::from(1);
    }

    if cli.location.is_dir() {
        for entry in WalkDir::new(cli.location)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file()
                && guess_file(&entry.path().to_path_buf())
                    .map(|f| f.contains(fmt))
                    .unwrap_or(false)
            {}
        }
    }

    ExitCode::from(0)
}
