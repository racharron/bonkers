impl crate::ThreadPool for threadpool::ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        self.execute(task)
    }
}
