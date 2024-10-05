use criterion::measurement::WallTime;
use criterion::{AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput};
use std::time::Duration;

pub const SIZES: [usize; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];
pub const MAX_THREADS: usize = 8;

pub fn run<T, NEW, BENCH>(c: &mut Criterion, name: &str, new: NEW, bench: BENCH)
where
    NEW: Fn(usize) -> T,
    BENCH: Fn(&mut BenchmarkGroup<WallTime>, usize, &T, BenchmarkId),
{
    let mut group = c.benchmark_group(name);
    group.warm_up_time(Duration::from_millis(500));
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for threads in 1..=MAX_THREADS {
        let pool = new(threads);
        for size in SIZES {
            group.throughput(Throughput::Elements(size as u64));
            let benchmark_id = BenchmarkId::new(if threads == 1 { "1-thread".to_string() } else { format!("{threads}-threads") }, size);
            bench(&mut group, size, &pool, benchmark_id);
        }
    }
    group.finish();
}
