use crate::rom_format::RomFormat;
use std::{
    fs::{copy, remove_file},
    path::PathBuf,
    process::Command,
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
}

impl Converter {
    pub fn new() -> Self {
        Self {
            thread_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn finish(&self) {
        while self.thread_count.load(Ordering::Relaxed) > 0 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn convert(&self, file: &PathBuf, format: RomFormat) {
        while self.thread_count.load(Ordering::Relaxed) >= num_cpus::get() {
            std::thread::sleep(Duration::from_millis(50));
        }

        let ptr = Arc::clone(&self.thread_count);
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

            let in_file = if format.contains(RomFormat::BIN) {
                p.parent().unwrap().join(format!(
                    "{}.{}",
                    p.file_stem().unwrap().to_str().unwrap(),
                    "cue",
                ))
            } else {
                p
            };

            if format.contains(RomFormat::PSX) || format.contains(RomFormat::PS2) {
                let out_file = in_file.parent().unwrap().join(format!(
                    "{}.{}",
                    in_file.file_stem().unwrap().to_str().unwrap(),
                    "chd",
                ));

                let mut cmd = Command::new("chdman");
                cmd.current_dir(&std::env::current_dir().unwrap())
                    .arg("createcd")
                    .args(["-i", in_file.to_str().unwrap()])
                    .args(["-o", out_file.to_str().unwrap()]);

                println!("{:?}", cmd);
                let _ = cmd.output();
            }

            cleanup(files);

            ptr.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
