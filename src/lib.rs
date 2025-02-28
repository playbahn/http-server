use std::sync::mpsc::{self, Receiver, RecvError, Sender};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

/// A thread pool implementation
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<Sender<Job>>,
}

impl ThreadPool {
    /// Create a new ThreadPool
    ///
    /// `size` is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if `size` is `0`
    ///
    /// # Examples
    ///
    /// Creates a thread pool with 4 threads
    /// ```
    /// use hello::ThreadPool;
    ///
    /// let pool = ThreadPool::new(4);
    /// ```
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver: Arc<Mutex<Receiver<Job>>> = Arc::new(Mutex::new(receiver));
        let mut workers: Vec<Worker> = Vec::with_capacity(size);

        (0..size).for_each(|id| workers.push(Worker::new(id, Arc::clone(&receiver))));

        ThreadPool {
            workers,
            sender: Some(sender),
        }
    }

    /// Register a function or closure `f` to be run using any arbitrary thread from pool `self`,
    /// `f` gets run when a thread is free to pick up `f`
    pub fn execute<F: FnOnce() + Send + 'static>(&self, f: F) {
        match &self.sender {
            Some(sender) => sender
                .send(Box::new(f))
                .unwrap_or_else(|e| eprintln!("{e}")),
            None => eprintln!("Sender dead."),
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers {
            eprintln!("Shutting down worker {}", worker.id);
            
            if let Some(thread) = worker.thread.take() {
                // A `Worker` instance can panic because its `thread` field
                // calls `Job` [`Box<dyn FnOnce() + Send + 'static>`],
                // which is a construct foreign to this library
                if thread.join().is_err() {
                    panic!("Worker {} panicked while Dropping Threadpool", worker.id)
                }
            }
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<Receiver<Job>>>) -> Worker {
        let thread: JoinHandle<()> = std::thread::spawn(move || loop {
            let job: Result<Job, RecvError> = receiver
                .lock()
                .expect("Last thread holding lock panicked")
                .recv();

            match job {
                Ok(job) => {
                    eprintln!("Worker {id} got a job, executing.");
                    job();
                }
                Err(_) => {
                    eprintln!("Worker {id} disconnected, shutting down.");
                    break;
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
