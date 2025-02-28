use chrono::format::{DelayedFormat, StrftimeItems};

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::{fs::File, io::BufWriter};

#[allow(dead_code)]
#[derive(Debug)]
pub enum Level {
    Info,
    Debug,
    Warn,
    Error,
    Trace,
    Fatal,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_uppercase())
    }
}

#[macro_export]
macro_rules! now {
    () => {
        chrono::Local::now().format("%+")
    };
}

pub type Dts<'a> = DelayedFormat<StrftimeItems<'a>>;

/// Writes to log buffer (log file) and stderr.
pub fn log<M: std::fmt::Display>(ll: Level, msg: M, dts: Dts, lbuf: &Arc<Mutex<BufWriter<File>>>) {
    let mut lbuf = lbuf
        .lock()
        .inspect_err(|poison_err| eprintln!("{poison_err}"))
        .unwrap_or_else(|poison_err| poison_err.into_inner());
    let logtext = format!("{dts} {ll} {msg}\n");
    eprint!("{logtext}");
    if let Err(e) = lbuf.write_all(logtext.as_bytes()).and(lbuf.flush()) {
        eprintln!("could not write/flush to log: {}", e.kind())
    }
}
