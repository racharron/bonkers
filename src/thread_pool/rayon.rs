impl crate::ThreadPool for rayon_core::ThreadPool {
    fn run<T: FnOnce() + Send + Sync + 'static>(&self, task: T) {
        self.spawn(task)
    }
}
