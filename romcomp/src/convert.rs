use crate::rom_format::RomFormat;
use filesize::PathExt;
use humansize::{format_size, DECIMAL};
use std::{
    fs::{copy, remove_file},
    path::PathBuf,
    process::{Command, Stdio},
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
}

pub struct Converter {
    thread_count: Arc<AtomicUsize>,
    skipped_files: Arc<AtomicUsize>,
    processed_files: Arc<AtomicUsize>,
    input_file_size: Arc<AtomicUsize>,
    output_file_size: Arc<AtomicUsize>,
}

impl Converter {
    pub fn new() -> Self {
        Self {
            thread_count: Arc::new(AtomicUsize::new(0)),
            skipped_files: Arc::new(AtomicUsize::new(0)),
            processed_files: Arc::new(AtomicUsize::new(0)),
            input_file_size: Arc::new(AtomicUsize::new(0)),
            output_file_size: Arc::new(AtomicUsize::new(0)),
        }
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
            "Conversion finished:
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
            return;
        }

        while self.thread_count.load(Ordering::Relaxed) >= num_cpus::get() {
            std::thread::sleep(Duration::from_millis(50));
        }

        let t_ptr = Arc::clone(&self.thread_count);
        let p_ptr = Arc::clone(&self.processed_files);
        let is_ptr = Arc::clone(&self.input_file_size);
        let os_ptr = Arc::clone(&self.output_file_size);
        let p = file.clone();

        self.thread_count.fetch_add(1, Ordering::Relaxed);

        std::thread::spawn(move || {
            let prepare_files = |p: &PathBuf, f: RomFormat| -> Vec<(PathBuf, FileSource)> {
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

            let cleanup = |f: Vec<(PathBuf, FileSource)>| {
                for (file, source) in f.into_iter() {
                    if source == FileSource::Temporary {
                        let _ = remove_file(file);
                    }
                }
            };

            let files = prepare_files(&p, format);

            is_ptr.fetch_add(
                p.size_on_disk().unwrap().try_into().unwrap(),
                Ordering::Relaxed,
            );

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

            if format.contains(RomFormat::PSX) || format.contains(RomFormat::PS2) {
                let _ = Command::new("chdman")
                    .current_dir(&std::env::current_dir().unwrap())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .arg("createcd")
                    .args(["-i", in_file.to_str().unwrap()])
                    .args(["-o", out_file.to_str().unwrap()])
                    .status();
            }

            cleanup(files);

            os_ptr.fetch_add(
                out_file.size_on_disk().unwrap().try_into().unwrap(),
                Ordering::Relaxed,
            );
            p_ptr.fetch_add(1, Ordering::Relaxed);
            t_ptr.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
