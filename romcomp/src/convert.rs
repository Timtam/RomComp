use crate::rom_format::RomFormat;
use crossbeam_channel::Receiver;
use cue::cd::CD;
use duct::cmd;
use filesize::PathExt;
use humansize::{format_size, DECIMAL};
use lazy_regex::regex_replace;
use std::{
    fs::{copy, remove_dir, remove_file, rename, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tempfile::TempDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileSource {
    /// input to romcomp, not created by us
    Input,
    /// temporary, created by romcomp
    Temporary,
    /// the compression target, created during the conversion
    Output,
}

pub struct Converter {
    available_threads: usize,
    thread_count: Arc<AtomicUsize>,
    skipped_files: Arc<AtomicUsize>,
    processed_files: Arc<AtomicUsize>,
    input_file_size: Arc<AtomicUsize>,
    output_file_size: Arc<AtomicUsize>,
    verbose: bool,
    remove_after_compression: bool,
    flatten: bool,
    root_directory: PathBuf,
    interrupt: Receiver<()>,
    temp_dir: Arc<TempDir>,
}

impl Converter {
    pub fn new(root: &PathBuf, temp_dir: TempDir, threads: usize, interrupt: Receiver<()>) -> Self {
        Self {
            available_threads: threads,
            thread_count: Arc::new(AtomicUsize::new(0)),
            skipped_files: Arc::new(AtomicUsize::new(0)),
            processed_files: Arc::new(AtomicUsize::new(0)),
            input_file_size: Arc::new(AtomicUsize::new(0)),
            output_file_size: Arc::new(AtomicUsize::new(0)),
            verbose: false,
            remove_after_compression: false,
            flatten: false,
            root_directory: root.clone(),
            interrupt,
            temp_dir: Arc::new(temp_dir),
        }
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn remove_after_compression(mut self, remove: bool) -> Self {
        self.remove_after_compression = remove;
        self
    }

    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }

    pub fn get_output_file_name(file: &PathBuf, format: RomFormat) -> Option<PathBuf> {
        if format.contains(RomFormat::PlayStationX) || format.contains(RomFormat::PlayStation2) {
            Some(
                Path::new(
                    regex_replace!(r"iso|(cue(\.txt)?)$"i, file.to_str().unwrap(), "chd").as_ref(),
                )
                .to_path_buf(),
            )
        } else if format.contains(RomFormat::PlayStationPortable) {
            Some(file.parent().unwrap().join(format!(
                "{}.{}",
                file.file_stem().unwrap().to_str().unwrap(),
                "cso"
            )))
        } else if format.contains(RomFormat::NintendoWii) {
            Some(file.parent().unwrap().join(format!(
                "{}.{}",
                file.file_stem().unwrap().to_str().unwrap(),
                "rvz"
            )))
        } else if format.contains(RomFormat::Nintendo64) || format.contains(RomFormat::NintendoDS) {
            Some(
                Path::new(
                    regex_replace!(r"(nds|n64|v64|z64)$"i, file.to_str().unwrap(), "zip").as_ref(),
                )
                .to_path_buf(),
            )
        } else {
            None
        }
    }

    pub fn finish(&self) {
        while self.thread_count.load(Ordering::Relaxed) > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }

        let processed = self.processed_files.load(Ordering::Relaxed);
        let skipped = self.skipped_files.load(Ordering::Relaxed);
        let is = self.input_file_size.load(Ordering::Relaxed);
        let os = self.output_file_size.load(Ordering::Relaxed);

        println!(
            "Compression finished:
            \tProcessed files: {}, Skipped files: {}, Total: {}
            \tInput file size: {}, Output file size: {}
            \tSaved {} ({:.2}%)",
            processed,
            skipped,
            processed + skipped,
            &format_size(is, DECIMAL),
            &format_size(os, DECIMAL),
            &format_size(is - os, DECIMAL),
            100f64 - (os as f64 * 100f64 / is as f64)
        );
    }

    pub fn convert(&self, file: &PathBuf, format: RomFormat) {
        if Converter::get_output_file_name(file, format)
            .map(|f| f.is_file())
            .unwrap_or(false)
        {
            self.skipped_files.fetch_add(1, Ordering::Relaxed);
            if self.verbose {
                println!("Skipping {}: Target file already exists", file.display());
            }
            return;
        }

        let itrp = self.interrupt.clone();

        while self.thread_count.load(Ordering::Relaxed) >= self.available_threads {
            std::thread::sleep(Duration::from_millis(50));

            if !itrp.is_empty() {
                return;
            }
        }

        let t_ptr = Arc::clone(&self.thread_count);
        let p_ptr = Arc::clone(&self.processed_files);
        let is_ptr = Arc::clone(&self.input_file_size);
        let os_ptr = Arc::clone(&self.output_file_size);
        let p = file.clone();
        let rem = self.remove_after_compression;
        let verbose = self.verbose;
        let flatten = self.flatten;
        let root = self.root_directory.clone();
        let temp_dir = Arc::clone(&self.temp_dir);

        self.thread_count.fetch_add(1, Ordering::Relaxed);

        if self.verbose {
            println!("Beginning compression of {}...", file.display());
        }

        std::thread::spawn(move || {
            let prepare_files =
                |p: &PathBuf, f: RomFormat, verbose: bool| -> Vec<(PathBuf, FileSource)> {
                    if f.contains(RomFormat::BIN) {
                        let mut files = vec![(p.clone(), FileSource::Input)];

                        if p.file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .ends_with("cue.txt")
                        {
                            let new = Path::new(
                                regex_replace!(r"\.txt$"i, p.to_str().unwrap(), "").as_ref(),
                            )
                            .to_path_buf();
                            if verbose {
                                println!("Copy {} to {} temporarily", p.display(), new.display());
                            }

                            let _ = copy(p, &new);

                            files.push((new, FileSource::Temporary));
                        }

                        files.append(
                            &mut CD::parse_file(p.clone())
                                .unwrap()
                                .tracks()
                                .into_iter()
                                .map(|t| {
                                    (
                                        p.parent().unwrap().join(t.get_filename()),
                                        FileSource::Input,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        );

                        files
                    } else if format.contains(RomFormat::Nintendo64) {
                        let mut files = vec![(p.clone(), FileSource::Input)];
                        if !format.contains(RomFormat::Z64) {
                            files.push((
                                p.parent().unwrap().join(format!(
                                    "{}.{}",
                                    p.file_stem().unwrap().to_str().unwrap(),
                                    "z64"
                                )),
                                FileSource::Temporary,
                            ));
                        }
                        files
                    } else if format.contains(RomFormat::NintendoDS) {
                        let new = temp_dir.path().join(p.file_name().unwrap()).to_path_buf();

                        if verbose {
                            println!("Copy {} to {} temporarily", p.display(), new.display());
                        }

                        let _ = copy(p, &new);

                        vec![(p.clone(), FileSource::Input), (new, FileSource::Temporary)]
                    } else {
                        vec![(p.clone(), FileSource::Input)]
                    }
                };

            let cleanup = |f: Vec<(PathBuf, FileSource)>,
                           remove_after_compression: bool,
                           interrupted: bool,
                           verbose: bool| {
                for (file, source) in f.into_iter() {
                    if source == FileSource::Temporary {
                        if verbose {
                            println!("Deleting temporary file {}", file.display());
                        }

                        let _ = remove_file(file);
                    } else if source == FileSource::Input
                        && remove_after_compression
                        && !interrupted
                    {
                        if verbose {
                            println!("Deleting input file {}", file.display());
                        }

                        let _ = remove_file(file);
                    } else if source == FileSource::Output && interrupted {
                        if verbose {
                            println!("Deleting incomplete output file {}", file.display());
                        }

                        let _ = remove_file(file);
                    }
                }
            };

            let flatten_directories = |file: &PathBuf, root: &PathBuf, verbose: bool| {
                let mut dir = file.parent();

                while dir.is_some_and(|dir| {
                    dir.starts_with(root) && dir.read_dir().is_ok_and(|rd| rd.count() == 1)
                }) {
                    dir = dir.unwrap().parent();
                }

                if dir.is_some() && dir != file.parent() {
                    if verbose {
                        println!(
                            "Moving {} to {}",
                            file.display(),
                            dir.unwrap().join(file.file_name().unwrap()).display()
                        );
                    }

                    if let Err(e) = rename(file, dir.unwrap().join(file.file_name().unwrap())) {
                        if verbose {
                            println!("Error moving file: {:?}", e);
                        }
                        return;
                    }

                    let mut current = file.parent();
                    while current != dir && current.is_some() {
                        if verbose {
                            println!("Removing empty directory {}", current.unwrap().display());
                        }
                        if let Err(e) = remove_dir(current.as_ref().unwrap()) {
                            if verbose {
                                println!("Error removing directory: {:?}", e);
                            }
                            return;
                        }
                        current = current.unwrap().parent();
                    }
                }
            };

            let mut files = prepare_files(&p, format, verbose);

            let mut is: u64 = 0;

            for (f, s) in files.iter() {
                if *s == FileSource::Input {
                    is += f.size_on_disk().unwrap();
                }
            }

            let in_file = if format.contains(RomFormat::BIN) {
                Path::new(regex_replace!(r"\.txt$"i, p.to_str().unwrap(), "").as_ref())
                    .to_path_buf()
            } else {
                p
            };

            let out_file = Converter::get_output_file_name(&in_file, format).unwrap();
            let mut interrupted = false;

            files.push((out_file.clone(), FileSource::Output));

            let expression = if format.contains(RomFormat::PlayStationX)
                || format.contains(RomFormat::PlayStation2)
            {
                Some(cmd!(
                    "chdman",
                    "createcd",
                    "-i",
                    in_file.to_str().unwrap(),
                    "-o",
                    out_file.to_str().unwrap()
                ))
            } else if format.contains(RomFormat::PlayStationPortable) {
                Some(cmd!("maxcso", in_file.to_str().unwrap(),))
            } else if format.contains(RomFormat::Nintendo64) && !format.contains(RomFormat::Z64) {
                Some(cmd!("rom64", "convert", in_file.to_str().unwrap(),))
            } else if format.contains(RomFormat::NintendoDS) {
                Some(cmd!(
                    "BitButcher",
                    "-e",
                    files
                        .iter()
                        .find(|(_, s)| *s == FileSource::Temporary)
                        .unwrap()
                        .0
                        .to_str()
                        .unwrap(),
                ))
            } else if format.contains(RomFormat::NintendoWii) {
                Some(cmd!(
                    "dolphin-tool",
                    "convert",
                    "-b",
                    "131072",
                    "-c",
                    "zstd",
                    "-f",
                    "rvz",
                    "-i",
                    in_file.to_str().unwrap(),
                    "-l",
                    "5",
                    "-o",
                    out_file.to_str().unwrap(),
                ))
            } else {
                None
            };

            if let Some(e) = expression {
                let proc = e
                    .dir(std::env::current_dir().unwrap())
                    .stderr_capture()
                    .stdout_capture()
                    .start()
                    .unwrap();

                loop {
                    let status = proc.try_wait();
                    if status.as_ref().is_ok_and(|e| *e == None) {
                        std::thread::sleep(Duration::from_millis(50));
                        if !itrp.is_empty() {
                            interrupted = true;
                            let _ = proc.kill();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    } else if status
                        .as_ref()
                        .is_ok_and(|e| e.is_some_and(|e| e.status.success()))
                    {
                        break;
                    } else {
                        interrupted = true;
                        break;
                    }
                }
            }

            if !interrupted
                && (format.contains(RomFormat::Nintendo64)
                    || format.contains(RomFormat::NintendoDS))
            {
                let temp_file = &files
                    .iter()
                    .find(|(_, s)| *s == FileSource::Temporary)
                    .unwrap_or_else(|| {
                        &files.iter().find(|(_, s)| *s == FileSource::Input).unwrap()
                    })
                    .0;

                if verbose {
                    println!("Zipping {} to {}", temp_file.display(), out_file.display());
                }

                let mut ifh = File::open(&temp_file).unwrap();
                let ofh = File::create(&out_file).unwrap();

                let mut zip = ZipWriter::new(ofh);

                let _ = zip
                    .start_file(
                        temp_file.file_name().unwrap().to_str().unwrap(),
                        SimpleFileOptions::default()
                            .compression_method(CompressionMethod::Deflated),
                    )
                    .unwrap();

                let mut buf = [0_u8; 1024 * 1024];

                'reader: while let Ok(chunk) = ifh.read(&mut buf) {
                    if chunk == 0 {
                        break;
                    }

                    let mut offset: usize = 0;

                    while offset < chunk {
                        if !itrp.is_empty() {
                            interrupted = true;
                            break 'reader;
                        }

                        let written = zip.write(&buf[offset..chunk]);

                        if written.is_err() {
                            break 'reader;
                        }

                        offset += written.unwrap();
                    }
                }

                let _ = zip.flush();

                let _ = zip.finish();

                drop(ifh);
            }

            let os = out_file.size_on_disk().unwrap_or(0);

            cleanup(files, rem, interrupted, verbose);

            if flatten && !interrupted {
                flatten_directories(&out_file, &root, verbose);
            }

            if !interrupted {
                println!("Finished compression of {}", out_file.display());
                is_ptr.fetch_add(is.try_into().unwrap(), Ordering::Relaxed);
                os_ptr.fetch_add(os.try_into().unwrap(), Ordering::Relaxed);
                p_ptr.fetch_add(1, Ordering::Relaxed);
            } else {
                println!("Aborted compression of {}", out_file.display());
            }

            t_ptr.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
