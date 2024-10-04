use crate::ThreadPool;
use std::mem::take;
use std::sync::{Mutex, MutexGuard};
use std::thread::{Builder, JoinHandle};

/// A mock threadpool that simply creates a new OS thread for each task.
pub struct OsThreads {
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl OsThreads {
    /// Create a new threadpool.  This does not create any threads.
    pub fn new() -> Self {
        Self {
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Wait for all outstanding tasks to complete.  Will not wait for any new tasks added during
    /// this method call.
    pub fn join(&self) {
        for thread in take(&mut *self.threads.lock().unwrap()) {
            thread.join().unwrap();
        }
    }

    fn remove_done(&self) -> MutexGuard<Vec<JoinHandle<()>>> {
        let mut guard = self.threads.lock().unwrap();
        let old_threads = take(&mut *guard);
        let mut new_threads = Vec::new();
        for thread in old_threads {
            if !thread.is_finished() {
                new_threads.push(thread);
            }
        }
        *guard = new_threads;
        guard
    }
}

impl Default for OsThreads {
    fn default() -> Self {
        OsThreads {
            threads: Mutex::new(Vec::new()),
        }
    }
}

impl ThreadPool for OsThreads {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        let mut threads = self.remove_done();
        let count = threads.len();
        threads.push(Builder::new().name(format!("OsThreads Task {count}")).spawn(task).unwrap());
    }
}
