use crate::{AtomicOrd, Backoff};
use std::collections::VecDeque;
use std::mem::take;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicIsize, AtomicUsize};
use std::thread::{current, park, yield_now, Builder, JoinHandle, Thread};

pub trait ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T);
    fn yield_here(&self);
}

pub struct MyThreadPool {
    shared: Arc<MyThreadPoolShared>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

struct MyThreadPoolShared {
    tasks: Mutex<VecDeque<Box<dyn FnOnce() + Send + Sync + 'static>>>,
    running_workers: AtomicUsize,
    state: AtomicIsize,
    parked_on_shutdown: Mutex<Vec<Thread>>,
}

#[derive(Debug, PartialEq, Eq)]
#[repr(isize)]
enum MyThreadPoolState {
    Running = 0,
    Cancelling = -1,
    ShutDown = -2,
}

impl TryFrom<isize> for MyThreadPoolState {
    type Error = isize;

    fn try_from(value: isize) -> Result<Self, Self::Error> {
        if value == Self::Running as isize {
            Ok(Self::Running)
        } else if value == Self::Cancelling as isize {
            Ok(Self::Cancelling)
        } else if value == Self::ShutDown as isize {
            Ok(Self::ShutDown)
        } else {
            Err(value)
        }
    }
}

pub struct OsThreads {
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl MyThreadPool {
    pub fn with_threads(threads: NonZeroUsize) -> Self {
        let thread_count = threads.get();
        let shared = {
            Arc::new(MyThreadPoolShared {
                tasks: Mutex::new(VecDeque::new()),
                running_workers: AtomicUsize::new(thread_count),
                state: AtomicIsize::new(MyThreadPoolState::Running as _),
                parked_on_shutdown: Mutex::new(Vec::new()),
            })
        };
        let threads = (0..thread_count).map(|i| {
            let shared = shared.clone();
            Builder::new().name(Self::new_name(i)).spawn(move || {
                let mut backoff = Backoff::new();
                loop {
                    match shared.state.load(AtomicOrd::Acquire).try_into().unwrap() {
                        MyThreadPoolState::Running => {
                            let task = shared.tasks.lock().unwrap().pop_front();
                            if let Some(task) = task {
                                backoff = Backoff::new();
                                task();
                            } else if !backoff.snooze() {
                                yield_now();
                            }
                        }
                        MyThreadPoolState::Cancelling =>  {
                            let old = shared.running_workers.fetch_sub(1, AtomicOrd::AcqRel);
                            if old == 1 {
                                shared.parked_on_shutdown.lock().unwrap().pop().unwrap().unpark();
                            }
                            return;
                        }
                        MyThreadPoolState::ShutDown => unreachable!(),
                    }
                }
            }).unwrap()
        })
        .collect();
        Self {
            shared,
            threads: Mutex::new(threads),
        }
    }

    fn new_name(i: usize) -> String {
        if let Some(name) = current().name() {
            format!("{name}.worker[{i}]")
        } else {
            format!("worker[{i}]")
        }
    }

    pub fn shutdown(&self) {
        if
            self.shared.state.compare_exchange(
                MyThreadPoolState::Running as _,
                MyThreadPoolState::Cancelling as _,
                AtomicOrd::AcqRel,
                AtomicOrd::Acquire
            )
                == Err(MyThreadPoolState::Cancelling as _)
        {
            return
        }
        self.shared.parked_on_shutdown.lock().unwrap().push(current());
        loop {
            park();
            if self.shared.running_workers.load(AtomicOrd::Acquire) == 0 {
                if self.shared.state.load(AtomicOrd::Acquire) == MyThreadPoolState::Cancelling as isize {
                    return
                }
                let threads = take(&mut *self.threads.lock().unwrap());
                if threads.is_empty() {
                    continue
                }
                for thread in threads {
                    thread.join().unwrap();
                }
                self.shared.state.store(MyThreadPoolState::ShutDown as isize, AtomicOrd::Release);
                for thread in take(&mut *self.shared.parked_on_shutdown.lock().unwrap()) {
                    thread.unpark();
                }
                return
            }
        }
    }
}

impl ThreadPool for MyThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        self.shared.tasks.lock().unwrap().push_back(Box::new(task));
    }


    fn yield_here(&self) {
        let task = self.shared.tasks.lock().unwrap().pop_front();
        if let Some(task) = task {
            task();
        } else {
            yield_now()
        }
    }
}

impl OsThreads {
    pub fn new() -> Self {
        Self {
            threads: Mutex::new(Vec::new()),
        }
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
        let mut threads = self.threads.lock().unwrap();
        let count = threads.len();
        threads.push(
            Builder::new()
                .name(format!("OsThreads Task {count}"))
                .spawn(task)
                .unwrap()
        );
    }

    fn yield_here(&self) {
        yield_now()
    }
}
