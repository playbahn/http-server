use http_server::ThreadPool;

mod logging;
use logging::Level::*;
use logging::{log, Dts};

use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, BufReader, BufWriter, ErrorKind::*};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock};

/// The conditional flag used for checking if the server is to be shut down.
static RUNNING: AtomicBool = AtomicBool::new(true);
/// We prepare responses during server initialization.
static RESPONSES: OnceLock<HashMap<PathBuf, Vec<u8>>> = OnceLock::new();

struct Conn {
    stream: TcpStream,
    addr: SocketAddr,
}

fn main() {
    let lbuf: Arc<Mutex<BufWriter<File>>> = Arc::new(Mutex::new(BufWriter::new(
        File::options()
            .append(true)
            .create(true)
            .open("log")
            .unwrap_or_else(|e| panic!("cannot open logfile: {}", e.kind())),
    )));

    RESPONSES
        .set(HashMap::from_iter(
            std::fs::read_dir("contents")
                .unwrap_or_else(|e| panic!("cannot open directory \"contents\": {e}"))
                .map(|dirent| {
                    let dirent = dirent.expect("should not fail, dirp created by std").path();
                    let status = match dirent.to_str().expect("path should be valid Unicode") {
                        "contents/hello.html" => "HTTP/1.1 200 OK",
                        "contents/400.html" => "HTTP/1.1 400 Bad Request",
                        "contents/404.html" => "HTTP/1.1 404 Not Found",
                        "contents/500.html" => "HTTP/1.1 500 Internal Server Error",
                        _ => panic!(),
                    };
                    let body = std::fs::read_to_string(&dirent).expect("TOCTOU bug");
                    let len = body.len();
                    let reponse = format!("{status}\r\nContent-Length: {len}\r\n\r\n{body}");
                    (dirent, reponse.into_bytes())
                }),
        ))
        .expect("should not fail, no other threads running");

    let local_addr = std::fs::read_to_string("config").expect("config filename should not change");
    let listener = TcpListener::bind(&local_addr)
        .unwrap_or_else(|e| panic!("cannot bind to {local_addr}: {}", e.kind()));
    let pool = ThreadPool::new(4);

    let lbuf2 = Arc::clone(&lbuf);
    ctrlc::set_handler(move || {
        RUNNING.store(false, SeqCst);
        // Connecting to self is enough for accept() to unblock
        if let Err(e) = TcpStream::connect(&local_addr) {
            #[rustfmt::skip]
            log(Error, format!("could not connect to {local_addr}: {}", e.kind()), now!(), &lbuf2);
        }
    })
    .unwrap_or_else(|e| panic!("{e}"));

    log(Info, "initialization done", now!(), &lbuf);

    while RUNNING.load(SeqCst) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let conn_logbuf = Arc::clone(&lbuf);
                pool.execute(move || conn_handler(Conn { stream, addr }, conn_logbuf, now!()));
            }
            Err(e) => log(Error, format!("accept failed: {}", e.kind()), now!(), &lbuf),
        }
    }
    
    log(Info, "shutdown", now!(), &lbuf);
}

fn conn_handler(conn: Conn, lbuf: Arc<Mutex<BufWriter<File>>>, dts: Dts) {
    let (mut stream, addr) = (conn.stream, conn.addr);
    let request: Vec<String> = BufReader::new(&mut stream)
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
        None => eprintln!("{dts} INFO blank request"),
        Some(first) => {
            log(Info, format!("from {addr}: {}", request[0]), dts, &lbuf);

            let response = &RESPONSES
                .get()
                .expect("should be set by main during initialization")[&PathBuf::from(
                match &first[..] {
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

            if let Err(e) = stream.write_all(response).and(stream.flush()) {
                log(Error, format!("to {addr}: {}", e.kind()), now!(), &lbuf);
            }
        }
    }
}
