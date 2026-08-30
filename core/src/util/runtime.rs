use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;

static CORE_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Returns the process-wide Tokio runtime for TurboTransfer.
pub fn get_core_runtime() -> &'static Runtime {
    CORE_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .enable_all()
            .thread_name("tt-tokio-worker")
            .build()
            .expect("Failed to initialize TurboTransfer Tokio runtime")
    })
}

/// Spawns a task safely regardless of calling thread context.
///
/// If the calling thread is inside an active Tokio runtime, spawns on that runtime.
/// If called from an external thread (e.g. Android JNI thread, UI thread, standard OS thread),
/// dispatches to the global `CORE_RUNTIME` with zero panic.
pub fn spawn_task<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    if let Ok(handle) = Handle::try_current() {
        handle.spawn(future)
    } else {
        get_core_runtime().spawn(future)
    }
}

/// Runs a future to completion on the Tokio runtime.
///
/// If already on a Tokio thread, spawns on blocking thread pool to prevent reactor stall;
/// otherwise, blocks directly on the core runtime.
pub fn block_on_task<F: Future>(future: F) -> F::Output {
    if Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| get_core_runtime().block_on(future))
    } else {
        get_core_runtime().block_on(future)
    }
}
