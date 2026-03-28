//! Parallel computation scheduler using Rayon.
//!
//! Provides a thread pool wrapper and utilities for distributing
//! acoustic calculation tasks across all available CPU cores.

use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Progress callback type: (completed_points, total_points).
pub type ProgressCallback = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Configuration for the parallel scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Number of threads (0 = use all logical CPUs).
    pub num_threads: usize,
    /// Chunk size for work distribution. Smaller = finer-grained progress.
    pub chunk_size: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            num_threads: 0,
            chunk_size: 64,
        }
    }
}

/// Parallel computation scheduler.
pub struct ParallelScheduler {
    pool: rayon::ThreadPool,
    chunk_size: usize,
}

impl ParallelScheduler {
    /// Build a scheduler. If `num_threads == 0`, uses all available cores.
    pub fn new(config: SchedulerConfig) -> Self {
        let threads = if config.num_threads == 0 {
            num_cpus::get()
        } else {
            config.num_threads
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("Failed to build Rayon thread pool");
        Self { pool, chunk_size: config.chunk_size }
    }

    /// Map a pure function `f` over `inputs` in parallel, returning results
    /// in the same order. Reports progress via optional callback.
    pub fn map<I, O, F>(&self, inputs: Vec<I>, f: F, progress: Option<ProgressCallback>) -> Vec<O>
    where
        I: Send,
        O: Send,
        F: Fn(I) -> O + Send + Sync,
    {
        let total = inputs.len() as u64;
        let completed = Arc::new(AtomicU64::new(0));

        self.pool.install(|| {
            inputs
                .into_par_iter()
                .chunks(self.chunk_size)
                .flat_map(|chunk| {
                    let results: Vec<O> = chunk.into_iter().map(&f).collect();
                    if let Some(ref cb) = progress {
                        let done = completed.fetch_add(results.len() as u64, Ordering::Relaxed)
                            + results.len() as u64;
                        cb(done, total);
                    }
                    results
                })
                .collect()
        })
    }

    pub fn num_threads(&self) -> usize {
        self.pool.current_num_threads()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_doubles_values_in_order() {
        let sched = ParallelScheduler::new(SchedulerConfig { num_threads: 2, chunk_size: 10 });
        let input: Vec<i32> = (0..100).collect();
        let output = sched.map(input.clone(), |x| x * 2, None);
        let expected: Vec<i32> = input.iter().map(|&x| x * 2).collect();
        assert_eq!(output, expected);
    }

    #[test]
    fn progress_callback_reaches_100_percent() {
        use std::sync::Mutex;
        let last = Arc::new(Mutex::new((0u64, 0u64)));
        let last_clone = Arc::clone(&last);
        let cb: ProgressCallback = Arc::new(move |done, total| {
            *last_clone.lock().unwrap() = (done, total);
        });

        let sched = ParallelScheduler::new(SchedulerConfig::default());
        let _ = sched.map(vec![1i32; 50], |x| x + 1, Some(cb));
        let (done, total) = *last.lock().unwrap();
        assert_eq!(done, total);
    }
}
