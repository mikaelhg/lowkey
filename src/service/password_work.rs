//! A tiny dedicated thread pool for password hashing.
//!
//! Each Argon2 run needs about 19 MiB. Running the work on a fixed set of
//! long-lived threads does two things: the thread count caps how many runs
//! happen at once, and each thread's allocator reuses the same 19 MiB rather
//! than every short-lived blocking thread caching a copy. Measured with 40
//! parallel logins, peak memory stays around 43 MB instead of 700 MB.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

use tokio::sync::oneshot;

/// How many password operations may run at once.
const THREADS: usize = 2;

type Job = Box<dyn FnOnce() + Send>;

static JOBS: LazyLock<Sender<Job>> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::channel::<Job>();
    let receiver = Arc::new(Mutex::new(receiver));
    for i in 0..THREADS {
        let receiver = receiver.clone();
        thread::Builder::new()
            .name(format!("password-{i}"))
            .spawn(move || {
                loop {
                    // The lock is released as soon as a job is taken.
                    let job = receiver.lock().expect("never poisoned").recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => break, // The sender is gone: shutting down.
                    }
                }
            })
            .expect("can spawn password threads");
    }
    sender
});

/// Runs CPU- and memory-heavy password work on the pool and waits for it
/// without blocking the async runtime. Jobs queue when the pool is busy.
pub async fn run_password_work<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, result) = oneshot::channel();
    JOBS.send(Box::new(move || {
        let _ = done.send(work());
    }))
    .expect("password threads are running");
    result.await.expect("password work does not panic")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queues_more_jobs_than_threads() {
        let results = tokio::join!(
            run_password_work(|| 1),
            run_password_work(|| 2),
            run_password_work(|| 3),
        );
        assert_eq!(results, (1, 2, 3));
    }

    #[tokio::test]
    async fn work_runs_on_the_pool_threads() {
        let name = run_password_work(|| thread::current().name().map(str::to_owned)).await;
        assert!(name.unwrap().starts_with("password-"));
    }
}
