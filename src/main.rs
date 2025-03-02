mod log;
mod macros;
mod threadpool;

use threadpool::ThreadPool;

use clap::Parser;

use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, BufReader, BufWriter, ErrorKind::*};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock};

/// The conditional flag used for checking if the server is to be shut down.
static RUNNING: AtomicBool = AtomicBool::new(true);
/// We prepare responses during server initialization.
static RESPONSES: OnceLock<HashMap<PathBuf, Vec<u8>>> = OnceLock::new();

struct Connection {
    stream: TcpStream,
    addr: SocketAddr,
}

#[derive(Parser, Debug)]
#[command(version, about = "An HTTP/1.1 server", long_about = None)]
struct Config {
    /// Size of threadpool
    #[arg(short, long, default_value_t = 4)]
    threads: u8,
    /// Port that server will bind to
    #[arg(short, long, default_value_t = 7878)]
    port: u16,
}

fn main() {
    let config = Config::parse();
    let lbuf = Arc::new(Mutex::new(BufWriter::new(
        File::options()
            .append(true)
            .create(true)
            .open(".log")
            .unwrap_or_else(|e| panic!("cannot open logfile: {}", e.kind())),
    )));

    RESPONSES
        .set(HashMap::from_iter(
            std::fs::read_dir("contents")
                .unwrap_or_else(|e| logged_panic!(lbuf, "cannot open contents/: {}", e.kind()))
                .map(|dirent| {
                    let dirent = dirent
                        .unwrap_or_else(|e| logged_panic!(lbuf, "cant read dirent: {}", e.kind()))
                        .path();
                    let status = match dirent.to_str().expect("path should be valid Unicode") {
                        "contents/hello.html" => "HTTP/1.1 200 OK",
                        "contents/400.html" => "HTTP/1.1 400 Bad Request",
                        "contents/404.html" => "HTTP/1.1 404 Not Found",
                        "contents/500.html" => "HTTP/1.1 500 Internal Server Error",
                        _ => unreachable!(),
                    };
                    let body = std::fs::read_to_string(&dirent)
                        .unwrap_or_else(|e| logged_panic!(lbuf, "cannot read file: {}", e.kind()));
                    let len = body.len();
                    let reponse = format!("{status}\r\nContent-Length: {len}\r\n\r\n{body}");
                    (dirent, reponse.into_bytes())
                }),
        ))
        .expect("should succeed, no other threads running");

    let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port);
    let listener = TcpListener::bind(local_addr)
        .unwrap_or_else(|e| logged_panic!(lbuf, "cannot bind to {local_addr}: {}", e.kind()));
    let pool = ThreadPool::new(config.threads);

    let lbuf2 = Arc::clone(&lbuf);
    ctrlc::set_handler(move || {
        RUNNING.store(false, SeqCst);
        // Connecting to self is enough for accept() to unblock
        if let Err(e) = TcpStream::connect(local_addr) {
            logwarn!(lbuf2, "could not connect to {local_addr}: {}", e.kind());
            // lbuf2.warn("killing server process");
        }
    })
    .unwrap_or_else(|e| logged_panic!(lbuf, "could not register Ctrl-C handler: {e}"));

    loginfo!(lbuf, "initialization done");
    logdbg!(lbuf, "{config:?}");

    while RUNNING.load(SeqCst) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let lbuf = lbuf.clone();
                pool.execute(move || conn_handler(Connection { stream, addr }, lbuf));
            }
            Err(e) => logerr!(lbuf, "could not accept connection: {}", e.kind()),
        }
    }

    loginfo!(lbuf, "shutdown");
}

fn conn_handler(mut conn: Connection, lbuf: Arc<Mutex<BufWriter<File>>>) {
    let request: Vec<String> = BufReader::new(&conn.stream)
        .lines()
        // errors that might come up while parsing the request lines
        .map(|line| {
            line.unwrap_or_else(|e| match e.kind() {
                InvalidInput | InvalidData => String::from("400\r\n"),
                _ => String::from("500\r\n"),
            })
        })
        .take_while(|line| !line.is_empty())
        .collect();

    match request.iter().peekable().peek() {
        None => eprintln!("{} INFO blank request", chrono::Local::now().format("%+")),
        Some(request_line) => {
            loginfo!(lbuf, "from {}: {request_line}", conn.addr);

            let response = &RESPONSES
                .get()
                .expect("should be set by main during initialization")[&PathBuf::from(
                match &request_line[..] {
                    "GET / HTTP/1.1" => "contents/hello.html",
                    "GET /sleep HTTP/1.1" => {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        "contents/hello.html"
                    }
                    "400" => "contents/400.html",
                    "500" => "contents/500.html",
                    _ => "contents/404.html",
                },
            )];

            if let Err(e) = conn.stream.write_all(response).and(conn.stream.flush()) {
                logwarn!(
                    lbuf,
                    "Could not succesfully respond to {}: {}\n{}",
                    conn.addr,
                    e.kind(),
                    request
                        .iter()
                        .fold(String::new(), |lines, line| lines + line + "\n")
                );
            }
        }
    }
}
