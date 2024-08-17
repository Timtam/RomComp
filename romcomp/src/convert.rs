use crate::rom_format::RomFormat;
use crossbeam_channel::Receiver;
use duct::cmd;
use filesize::PathExt;
use humansize::{format_size, DECIMAL};
use std::{
    fs::{copy, remove_dir, remove_file, rename},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Copy, Eq, PartialEq)]
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
}

impl Converter {
    pub fn new(root: &PathBuf, threads: usize, interrupt: Receiver<()>) -> Self {
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
        if format.contains(RomFormat::PSX) || format.contains(RomFormat::PS2) {
            Some(file.parent().unwrap().join(format!(
                "{}.{}",
                file.file_stem().unwrap().to_str().unwrap(),
                "chd"
            )))
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

        self.thread_count.fetch_add(1, Ordering::Relaxed);

        if self.verbose {
            println!("Beginning compression of {}...", file.display());
        }

        std::thread::spawn(move || {
            let prepare_files =
                |p: &PathBuf, f: RomFormat, verbose: bool| -> Vec<(PathBuf, FileSource)> {
                    if f.contains(RomFormat::BIN) {
                        if p.parent()
                            .unwrap()
                            .join(format!(
                                "{}.{}",
                                p.file_stem().unwrap().to_str().unwrap(),
                                "cue.txt"
                            ))
                            .is_file()
                        {
                            if verbose {
                                println!(
                                    "Copy {} to {} temporarily",
                                    p.parent()
                                        .unwrap()
                                        .join(format!(
                                            "{}.{}",
                                            p.file_stem().unwrap().to_str().unwrap(),
                                            "cue.txt"
                                        ))
                                        .display(),
                                    p.parent()
                                        .unwrap()
                                        .join(format!(
                                            "{}.{}",
                                            p.file_stem().unwrap().to_str().unwrap(),
                                            "cue"
                                        ))
                                        .display()
                                );
                            }
                            let _ = copy(
                                p.parent().unwrap().join(format!(
                                    "{}.{}",
                                    p.file_stem().unwrap().to_str().unwrap(),
                                    "cue.txt"
                                )),
                                p.parent().unwrap().join(format!(
                                    "{}.{}",
                                    p.file_stem().unwrap().to_str().unwrap(),
                                    "cue"
                                )),
                            );
                            vec![
                                (p.clone(), FileSource::Input),
                                (
                                    p.parent().unwrap().join(format!(
                                        "{}.{}",
                                        p.file_stem().unwrap().to_str().unwrap(),
                                        "cue.txt"
                                    )),
                                    FileSource::Input,
                                ),
                                (
                                    p.parent().unwrap().join(format!(
                                        "{}.{}",
                                        p.file_stem().unwrap().to_str().unwrap(),
                                        "cue"
                                    )),
                                    FileSource::Temporary,
                                ),
                            ]
                        } else {
                            vec![
                                (p.clone(), FileSource::Input),
                                (
                                    p.parent().unwrap().join(format!(
                                        "{}.{}",
                                        p.file_stem().unwrap().to_str().unwrap(),
                                        "cue"
                                    )),
                                    FileSource::Input,
                                ),
                            ]
                        }
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
            let is = p.size_on_disk().unwrap();

            let in_file = if format.contains(RomFormat::BIN) {
                p.parent().unwrap().join(format!(
                    "{}.{}",
                    p.file_stem().unwrap().to_str().unwrap(),
                    "cue",
                ))
            } else {
                p
            };

            let out_file = Converter::get_output_file_name(&in_file, format).unwrap();
            let mut interrupted = false;

            files.push((out_file.clone(), FileSource::Output));

            if format.contains(RomFormat::PSX) || format.contains(RomFormat::PS2) {
                let proc = cmd!(
                    "chdman",
                    "createcd",
                    "-i",
                    in_file.to_str().unwrap(),
                    "-o",
                    out_file.to_str().unwrap()
                )
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

            let os = out_file.size_on_disk().unwrap();

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
