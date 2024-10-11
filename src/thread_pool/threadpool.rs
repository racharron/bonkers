use crate::{when, LockCollection};

impl crate::ThreadPool for threadpool::ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        self.execute(task)
    }
}

impl crate::Runner for threadpool::ThreadPool {
    type ThreadPool = Self;

    fn when<L, T>(&self, cowns: L, thunk: T)
    where
        L: LockCollection,
        T: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static,
    {
        when(self, cowns, thunk);
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}
