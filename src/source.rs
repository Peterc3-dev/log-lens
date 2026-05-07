use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

/// Messages from source readers
pub enum SourceMsg {
    Line { content: String, source_idx: usize },
    Error { msg: String, source_idx: usize },
    Eof { source_idx: usize },
}

/// Start reading from stdin in a background thread
pub fn read_stdin(tx: mpsc::Sender<SourceMsg>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let reader = BufReader::new(stdin.lock());
        for line in reader.lines() {
            match line {
                Ok(content) => {
                    if tx.send(SourceMsg::Line { content, source_idx: 0 }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(SourceMsg::Error {
                        msg: e.to_string(),
                        source_idx: 0,
                    });
                    break;
                }
            }
        }
        let _ = tx.send(SourceMsg::Eof { source_idx: 0 });
    });
}

/// Start reading from a file. If follow=true, keep tailing after EOF.
pub fn read_file(path: PathBuf, source_idx: usize, follow: bool, tx: mpsc::Sender<SourceMsg>) {
    thread::spawn(move || {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                let _ = tx.send(SourceMsg::Error {
                    msg: format!("Cannot open {}: {}", path.display(), e),
                    source_idx,
                });
                return;
            }
        };

        let mut reader = BufReader::new(file);
        let mut line_buf = String::new();

        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => {
                    // EOF
                    if follow {
                        // Sleep and retry (tail -f behavior)
                        thread::sleep(std::time::Duration::from_millis(100));

                        // Reopen to check for truncation/rotation
                        // (simplified: just keep reading from same handle)
                        continue;
                    } else {
                        let _ = tx.send(SourceMsg::Eof { source_idx });
                        break;
                    }
                }
                Ok(_) => {
                    // Strip trailing newline
                    let content = line_buf.trim_end_matches('\n').trim_end_matches('\r').to_string();
                    if tx.send(SourceMsg::Line { content, source_idx }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    // Non-UTF8 or other error — try to continue
                    let _ = tx.send(SourceMsg::Error {
                        msg: format!("Read error on {}: {}", path.display(), e),
                        source_idx,
                    });
                    if !follow {
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });
}

/// Start reading from a generic reader (used for piped stdin with follow)
pub fn read_reader<R: Read + Send + 'static>(
    reader: R,
    source_idx: usize,
    tx: mpsc::Sender<SourceMsg>,
) {
    thread::spawn(move || {
        let buf_reader = BufReader::new(reader);
        for line in buf_reader.lines() {
            match line {
                Ok(content) => {
                    if tx.send(SourceMsg::Line { content, source_idx }).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(SourceMsg::Error {
                        msg: e.to_string(),
                        source_idx,
                    });
                    break;
                }
            }
        }
        let _ = tx.send(SourceMsg::Eof { source_idx });
    });
}
