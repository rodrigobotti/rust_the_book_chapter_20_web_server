use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

impl ThreadPool {
    /// Create a new ThreadPool.
    ///
    /// The size is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)))
        }

        ThreadPool {
            workers,
            sender: Some(sender),
        }
    }
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);

        self.sender.as_ref().unwrap().send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers {
            println!("Shutting down worker {}", worker.id);

            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

struct Worker {
    id: usize,
    // using Option<JoinHandle> instead of JoinHandle
    // so we can move it out of the worker instance (using the take method) and call the join method
    // during cleanup (threadpool's implmentation)
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        // Note: If the operating system can’t create a thread because there aren’t enough system resources, thread::spawn will panic.
        // That will cause our whole server to panic, even though the creation of some threads might succeed.
        // For simplicity’s sake, this behavior is fine, but in a production thread pool implementation,
        // you’d likely want to use std::thread::Builder and its spawn method that returns Result instead.

        let thread = thread::spawn(move || loop {
            // the lock is immediately dropped here because with let,
            // any temporary values used in the expression on the right hand side of the equals sign are immediately dropped
            // when the let statement ends.
            let message = receiver.lock().unwrap().recv();

            match message {
                Ok(job) => {
                    println!("Worker {id} got a job; executing.");
                    job();
                }
                Err(_) => {
                    println!("Worker {id} disconnected; shutting down.");
                    break;
                }
            }
        });

        // `while let` (and `if let` and `match`) does not drop temporary values until the end of the associated block.
        // In the following commented code, the lock remains held for the duration of the call to job(),
        // meaning other workers cannot receive jobs.
        //
        // let thread = thread::spawn(move || {
        //     while let Ok(job) = receiver.lock().unwrap().recv() {
        //         println!("Worker {id} got a job; executing.");
        //         job();
        //     }
        // });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}
