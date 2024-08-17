mod convert;
mod rom_format;
mod search;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use convert::Converter;
use crossbeam_channel::{bounded, Receiver};
use rom_format::RomFormat;
use search::guess_file;
use std::{
    fs::canonicalize,
    io::ErrorKind,
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
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
    verbose: bool,

    /// how many conversions should be running in parallel?
    /// default is the amount of available CPU cores

    #[arg(short, long, action, default_value_t = num_cpus::get())]
    threads: usize,

    /// delete input files after compression

    #[arg(short = 'R', long = "remove", action)]
    remove_after_compression: bool,

    /// flatten directory structure by moving the output file into parent directories until its not the only file in the directory anymore.
    /// can only be used in conjunction with --remove,
    /// can only be used if the input location is a directory, flatten will never move files outside that given location

    #[arg(short, long, action)]
    flatten: bool,
}

#[derive(ValueEnum, Clone, Eq, PartialEq, Debug)]
enum SourceRomFormat {
    Psx,
    Ps2,
    Psp,
}

fn ctrl_channel() -> Result<Receiver<()>> {
    let (sender, receiver) = bounded(100);

    ctrlc::set_handler(move || {
        let _ = sender.send(());
    })?;

    Ok(receiver)
}

fn main() -> Result<ExitCode> {
    let ctrl_c_events = ctrl_channel()?;
    let cli = Cli::parse();

    let location = canonicalize(cli.location.clone());

    if !location.as_ref().map(|l| l.exists()).unwrap_or(false) {
        println!("The path {} doesn't exist.", cli.location.to_str().unwrap());
        return Ok(ExitCode::from(1));
    }

    let location = location.unwrap();

    let fmt = match cli.format {
        SourceRomFormat::Psx => RomFormat::PSX,
        SourceRomFormat::Ps2 => RomFormat::PS2,
        SourceRomFormat::Psp => RomFormat::PSP,
    };

    if cli.flatten && !cli.remove_after_compression {
        println!("--flatten can only be used in conjunction with the --remove parameter.");
        return Ok(ExitCode::from(1));
    }

    if cli.flatten && !location.is_dir() {
        println!("--flatten can only be used if the input location is a directory");
        return Ok(ExitCode::from(1));
    }

    if cli.format == SourceRomFormat::Psx || cli.format == SourceRomFormat::Ps2 {
        match Command::new("chdman")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Err(e) => {
                if let ErrorKind::NotFound = e.kind() {
                    println!("You'll need to have CHDMAN available on your PATH if you want to convert these ROMs. Please run this application from Docker or install CHDMAN manually and try again.");
                    return Ok(ExitCode::from(2));
                }
            }
            _ => (),
        }
    }

    if location.is_file()
        && !guess_file(&location)
            .map(|f| f.contains(fmt))
            .unwrap_or(false)
    {
        println!(
            "The input file isn't recognized as proper file format for a {:?} rom",
            cli.format
        );
        return Ok(ExitCode::from(1));
    }

    let converter = Converter::new(&location, cli.threads, ctrl_c_events.clone())
        .verbose(cli.verbose)
        .remove_after_compression(cli.remove_after_compression)
        .flatten(cli.flatten);

    println!(
        "Start ROM compression with {} simultaneous processes",
        cli.threads
    );

    if location.is_dir() {
        for entry in WalkDir::new(location).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let guess = guess_file(&entry.path().to_path_buf());
                if guess.is_some_and(|f| f.contains(fmt)) {
                    if !ctrl_c_events.is_empty() {
                        break;
                    }
                    converter.convert(
                        &entry.path().to_path_buf(),
                        (guess.unwrap() & RomFormat::FILE_FORMATS) | fmt,
                    );
                }
            }
        }
    } else {
        converter.convert(
            &location,
            (guess_file(&location).unwrap() & RomFormat::FILE_FORMATS) | fmt,
        );
    }

    converter.finish();

    Ok(ExitCode::from(0))
}
