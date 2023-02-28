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
    /// Creates a new thread pool capable of executing `size` number of jobs concurrently.
    ///
    /// The size is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    ///
    /// # Examples
    /// ```
    /// use hello::ThreadPool;
    ///
    /// let pool = ThreadPool::new(4);
    ///```
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

    /// Executes the function `f` on a thread in the pool.
    ///
    /// # Examples
    /// ```
    /// use std::sync::atomic::AtomicI32;
    /// use std::sync::atomic::Ordering::SeqCst;
    /// use std::sync::Arc;
    /// use hello::ThreadPool;
    ///
    /// let counter = Arc::new(AtomicI32::new(0));
    /// {
    ///     let size = 4;
    ///     let pool = ThreadPool::new(size);
    ///
    ///     for _ in 0..size {
    ///         let counter = Arc::clone(&counter);
    ///         pool.execute(move || {
    ///             counter.fetch_add(1, SeqCst);
    ///         });
    ///     }
    /// }
    ///
    /// assert_eq!(counter.load(SeqCst), 4);
    /// ```
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);

        self.sender
            .as_ref()
            .unwrap()
            .send(job)
            .expect("ThreadPool::execute unable to send job into queue.");
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_works() {
        let size = 4;
        let pool = ThreadPool::new(size);
        let (tx, rx) = mpsc::channel();
        let expected = vec![1; size];

        for _ in 0..size {
            let tx = tx.clone();
            pool.execute(move || tx.send(1).unwrap());
        }
        let res: Vec<i32> = rx.iter().take(size).collect();

        assert_eq!(res, expected);
    }
}
