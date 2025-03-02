use std::io::Write;
use std::sync::{Arc, Mutex};
use std::{fs::File, io::BufWriter};

#[derive(Debug)]
pub enum LogLevel {
    #[allow(dead_code)]
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_uppercase())
    }
}

pub fn log(buf: &Arc<Mutex<BufWriter<File>>>, logtext: String) {
    let mut buf = buf.lock().unwrap_or_else(|poison_err| {
        eprintln!("{poison_err}");
        poison_err.into_inner()
    });

    eprint!("{logtext}");

    if let Err(e) = buf.write_all(logtext.as_bytes()).and(buf.flush()) {
        eprintln!("could not write/flush to log: {}", e.kind())
    }
}
