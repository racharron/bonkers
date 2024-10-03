use std::collections::VecDeque;
use std::num::NonZeroUsize;
use crossbeam_queue::SegQueue;
use loom::lazy_static;
use crate::*;

/// We can't test scoped
pub struct ThreadPool {
    pub(crate) min_threads: usize,
    pub(crate) max_threads: NonZeroUsize,
    pub(crate) ready_tasks: Mutex<VecDeque<Task>>,
    pub(crate) active_threads: AtomicIsize,
    pub(crate) parked_workers: Mutex<Vec<Thread>>,
    pub(crate) parked_waiting: Mutex<Vec<Thread>>,
    pub(crate) worker_threads: RwLock<Vec<JoinHandle<()>>>,
}

impl ThreadPool {
    pub fn new(min_threads: usize, max_threads: NonZeroUsize) -> Result<Self, NonZeroUsize> {
        match NonZero::new(min_threads.saturating_sub(max_threads.get())) {
            None => {
                Ok(Self {
                    min_threads,
                    max_threads,
                    ready_tasks: Mutex::new(VecDeque::new()),
                    active_threads: AtomicIsize::new(0),
                    parked_workers: Mutex::new(Vec::new()),
                    parked_waiting: Mutex::new(Vec::new()),
                    worker_threads: RwLock::new(Vec::new()),
                })
            }
            Some(over) => Err(over),
        }
    }
}
