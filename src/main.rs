use http_server::ThreadPool;

use std::fs::File;
use std::{fs, sync};
use std::io::{prelude::*, BufReader, BufWriter, ErrorKind};
use std::net::{TcpListener, TcpStream};
use std::sync::{atomic::AtomicBool, Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use ctrlc;
use chrono::prelude::{DateTime, Local};

fn main() {
    let server_init_datetime: DateTime<Local> = Local::now();

    let logbuf: Arc<Mutex<BufWriter<File>>> =
        Arc::new(Mutex::new(BufWriter::new(match File::options()
            .write(true).append(true).create(true)
            .open("server.log") {
                Ok(file) => file,
                Err(e) => panic!("Error opening log file [server.log]: {:#?}", e.kind()),
            })));

    let sigint_handler_logbuf: Arc<Mutex<BufWriter<File>>> = Arc::clone(&logbuf);

    {
        match logbuf.lock() {
            Ok(mut logbuf) => {
                if let Err(e) = logbuf.write_all(
                    format!("[{}] SERVER INIT\n", server_init_datetime).as_bytes()
                ) {
                    eprintln!("SERVER INIT LOG ERROR: {:#?}", e.kind());
                } else {
                    if let Err(e) = logbuf.flush() {
                        eprintln!("SERVER INIT LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                    };
                };
            },
            Err(_) => panic!("UNEXPECTED ERROR WHILE LOCKING ON LOGFILE BUFFER"),
        };
    }

    let server_socket_addr: &str = "127.0.0.1:7878";
    let tcp_listener: TcpListener = match TcpListener::bind(server_socket_addr) {
        Ok(tcp_listener) => tcp_listener,
        Err(e) => panic!("BIND ERROR: {:#?}", e.kind()),
    };

    let pool: ThreadPool = ThreadPool::new(4);

    let sigint_retrieve: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let sigint_set: Arc<AtomicBool> = Arc::clone(&sigint_retrieve);
    
    ctrlc::set_handler(move || {
        sigint_set.store(true, sync::atomic::Ordering::SeqCst);

        let mut sigint_handler_logbuf: MutexGuard<'_, BufWriter<File>> = match sigint_handler_logbuf.lock() {
            Ok(logbuf) => logbuf,
            Err(poison_err) => {
                eprintln!("SIGINT handler: BufWriter<server.log> PoisonError. Logging nonetheless.");
                poison_err.into_inner()
            },
        };

        // connect to same socket our server is listening on, so that we send a dupe request,
        // after obviously setting loop condition variable `sigint_retrieve` to false
        match TcpStream::connect(server_socket_addr) {
            Ok(mut send_stream) => {
                let dupe_rqst: &str = "SIGINT HANDLER SERVER SHUTDOWN REQUEST\n";

                match send_stream.write_all(
                    dupe_rqst.as_bytes()
                ) {
                    Ok(()) => {
                        let dupe_rqst_log_msg_success_datetime: DateTime<Local> = Local::now();
                        let dupe_rqst_log_msg_success: String = format!("SIGINT handler: Dupe request sent for server shutdown:\n\t{dupe_rqst}\n\n");

                        if let Err(e) = sigint_handler_logbuf.write_all(
                            format!("[{dupe_rqst_log_msg_success_datetime}] {dupe_rqst_log_msg_success}").as_bytes()
                        ) {
                            eprintln!("SIGINT handler: ERROR LOGGING DUPE REQUEST MESSAGE: {:#?}", e.kind());
                        } else {
                            if let Err(e) = sigint_handler_logbuf.flush() {
                                eprintln!("SIGINT handler: FLUSH ERROR WHILE LOGGING SERVER SHUTDOWN REQUEST: {:#?}", e.kind());
                            };
                        };

                        print!("[{dupe_rqst_log_msg_success_datetime}] {dupe_rqst_log_msg_success}");
                    },
                    Err(e) => {
                        let dupe_rqst_log_msg_failure_datetime: DateTime<Local> = Local::now();
                        let dupe_rqst_log_msg_failure: String = format!(
                            "[{dupe_rqst_log_msg_failure_datetime}] SIGINT handler: Couldn't send server shutdown request: {:#?}\n", e.kind()
                        );
                        
                        if let Err(e) = sigint_handler_logbuf.write_all(
                            dupe_rqst_log_msg_failure.as_bytes()
                        ) {
                            eprintln!("SIGINT handler: ERROR LOGGING FAILED DUPE REQUEST MESSAGE: {:#?}", e.kind());
                        } else {
                            if let Err(e) = sigint_handler_logbuf.flush() {
                                eprintln!("SIGINT handler: FLUSH ERROR WHILE LOGGING FAILED SERVER SHUTDOWN REQUEST: {:#?}", e.kind());
                            };
                        };

                        eprintln!("[{dupe_rqst_log_msg_failure_datetime}] {dupe_rqst_log_msg_failure}");
                    },
                };
            },
            Err(e) => {
                let localhost_conn_err_datetime: DateTime<Local> = Local::now();
                let localhost_conn_err: String = format!("[{localhost_conn_err_datetime}] SIGINT handler: ERROR CONNECTING TO LOCALHOST: {:#?}\n", e.kind());
                
                if let Err(e) = sigint_handler_logbuf.write_all(
                    localhost_conn_err.as_bytes()
                ) {
                    eprintln!("SIGINT handler: ERROR LOGGING FAILED DUPE REQUEST MESSAGE: {:#?}", e.kind());
                } else {
                    if let Err(e) = sigint_handler_logbuf.flush() {
                        eprintln!("SIGINT handler: FLUSH ERROR WHILE LOGGING FAILED SERVER SHUTDOWN REQUEST: {:#?}", e.kind());
                    };
                };

                eprintln!("[{localhost_conn_err_datetime}] {localhost_conn_err}");
            },
        };
    }).expect(&format!("Error setting SIGINT handler."));

    for stream in tcp_listener.incoming() {
        let request_datetime: DateTime<Local> = Local::now();
        
        if sigint_retrieve.load(sync::atomic::Ordering::SeqCst) {
            // loop exit ultimately ends main(), calling `Drop for ThreadPool`
            break;
        };

        let stream: TcpStream = match stream {
            Ok(tcp_stream) => tcp_stream,

            Err(e) => {
                let tcp_stream_encountered_io_datetime: DateTime<Local> = Local::now();

                let err: String = format!("[{tcp_stream_encountered_io_datetime}] ENCOUNTERED IO ERROR: {:#?}", e.kind());
                eprintln!("[{tcp_stream_encountered_io_datetime}] {err}");

                let mut logbuf: MutexGuard<'_, BufWriter<File>> = match logbuf.lock() {
                    Ok(logbuf) => logbuf,
                    Err(poison_err) => {
                        eprintln!("TcpStream iterator: BufWriter<server.log> PoisonError. Logging nonetheless.");
                        poison_err.into_inner()
                    },
                };

                if let Err(e) = logbuf.write_all(
                    format!("{err} in incoming TcpStream\n").as_bytes()
                ) {
                    eprintln!("Incoming TcpStream encountered IO error LOG ERROR: {:#?}", e.kind());
                } else {
                    if let Err(e) = logbuf.flush() {
                        eprintln!("Incoming TcpStream encountered IO error LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                    };
                };

                continue;
            },
        };

        let logbuf: Arc<Mutex<BufWriter<File>>> = Arc::clone(&logbuf);

        pool.execute(move || {
            handle_connection(stream, logbuf, request_datetime);
        });
    }

    println!("[{}] Shutting down.", Local::now());
}

fn handle_connection(mut stream: TcpStream, logbuf: Arc<Mutex<BufWriter<File>>>, request_datetime: DateTime<Local>) {
    let buf_reader: BufReader<&mut TcpStream> = BufReader::new(&mut stream);

    let http_request: Vec<String> = buf_reader
        .lines()
        // errors that might come up while parsing the request line(s)
        .map(|result| match result {
            Ok(request_line) => request_line,
            Err(e) => {
                match e.kind() {
                    ErrorKind::InvalidInput | ErrorKind::InvalidData | ErrorKind::UnexpectedEof =>
                        String::from("CHECKED BY SERVER: 400 Bad Request\n"),
                    _ => String::from("CHECKED BY SERVER: 500 Internal Server Error\n"),
                }
            },
        })
        .take_while(|line| !line.is_empty())
        .collect();
    
    match http_request.iter().peekable().peek() {
        None => eprintln!("[{request_datetime}] REQUEST ERROR: Blank Request"),

        Some(request_line) => {
            let (status_line, filename): (&str, &str) = match &request_line[..] {
                "GET / HTTP/1.1" => ("HTTP/1.1 200 OK", "hello.html"),
                
                "GET /sleep HTTP/1.1" => {
                    thread::sleep(Duration::from_secs(5));
                    ("HTTP/1.1 200 OK", "hello.html")
                },

                "CHECKED BY SERVER: 400 Bad Request" => ("HTTP/1.1 400 Bad Request", "400.html"),
                
                "CHECKED BY SERVER: 500 Internal Server Error" => ("HTTP/1.1 500 Internal Server Error", "500.html"),

                _ => ("HTTP/1.1 404 Not Found", "404.html"),
            };

            let contents: String = fs::read_to_string(filename)
                .expect("Reading from local files shouldn't result in any `std::io::Error`");
            let length: usize = contents.len();

            let response: String = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");

            {
                let mut logbuf: MutexGuard<'_, BufWriter<File>> = match logbuf.lock() {
                    Ok(logbuf) => logbuf,
                    Err(poison_err) => {
                        eprintln!("Connection handler: BufWriter<server.log> PoisonError. Logging nonetheless.");
                        poison_err.into_inner()
                    },
                };

                if let Err(e) = logbuf.write_all(
                    format!("[{request_datetime}] Connection handler: Request:\n").as_bytes()
                ) {
                    eprintln!("Connection handler: HTTP REQUEST LOG ERROR: {:#?}", e.kind());
                } else {
                    if let Err(e) = logbuf.flush() {
                        eprintln!("Connection handler: HTTP REQUEST LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                    };
                };

                print!("[{request_datetime}] Connection handler: Request:\n");

                for line in &http_request {
                    if let Err(e) = logbuf.write_all(
                        format!("{line}\n").as_bytes()
                    ) {
                        eprintln!("Connection handler: HTTP REQUEST LOG ERROR: {:#?}", e.kind());
                    } else {
                        if let Err(e) = logbuf.flush() {
                            eprintln!("Connection handler: HTTP REQUEST LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                        };
                    };

                    println!("{line}");
                }

                if let Err(e) = logbuf.write_all(
                    "\n".as_bytes()
                ) {
                    eprintln!("Connection handler: HTTP REQUEST LOG ERROR: {:#?}", e.kind());
                } else {
                    if let Err(e) = logbuf.flush() {
                        eprintln!("Connection handler: HTTP REQUEST LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                    };
                };

                match stream.write_all(
                    response.as_bytes()
                ) {
                    Ok(()) => {
                        let response_datetime: DateTime<Local> = Local::now();
                        let response_ok: String = format!("[{response_datetime}] Connection handler: Response:\n{}\n", response);
                        
                        if let Err(e) = logbuf.write_all(
                            response_ok.as_bytes()
                        ) {
                            eprintln!("Connection handler: HTTP RESPONSE LOG ERROR: {:#?}", e.kind());
                        } else {
                            if let Err(e) = logbuf.flush() {
                                eprintln!("Connection handler: HTTP RESPONSE LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                            };
                        };

                        print!("\n{response_ok}");
                    },
                    Err(e) => {
                        let response_err_datetime: DateTime<Local> = Local::now();
                        let response_err: String = format!("[{response_err_datetime}] Connection handler: Error writing response to the stream: {:#?}\n", e.kind());

                        if let Err(e) = logbuf.write_all(
                            response_err.as_bytes()
                        ) {
                            eprintln!("Connection handler: HTTP FAILED RESPONSE LOG ERROR: {:#?}", e.kind());
                        } else {
                            if let Err(e) = logbuf.flush() {
                                eprintln!("Connection handler: HTTP FAILED RESPONSE LOG WRITE FLUSH ERROR: {:#?}", e.kind());
                            };
                        };

                        eprint!("\n{response_err}");
                    },
                };
            };
        },
    };
}
