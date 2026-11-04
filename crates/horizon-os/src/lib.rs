//! The operating system as the game sees it: files, clocks and the process's
//! folder. On native these are std's own; in the browser, where std has no
//! filesystem and its clocks panic, they are a virtual filesystem (backed by
//! files fetched from the page's origin) and the browser's clocks, with the
//! same API so call sites read the same on both.

// The browser build runs its threads as Web Workers on shared memory, which
// std only supports when it is rebuilt with atomics (web/build.sh).
#[cfg(all(target_arch = "wasm32", not(target_feature = "atomics")))]
compile_error!("the browser build needs threads: build it with web/build.sh");

pub mod env;
pub mod fs;
#[cfg(target_arch = "wasm32")]
pub mod net;
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub mod workers;

/// Waits for `future`. On native, as long as it takes; on the web, where the
/// page's only thread can't wait for the browser, `None` unless the future is
/// ready on its first poll.
pub fn block_on<F: std::future::Future>(future: F) -> Option<F::Output> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Some(pollster::block_on(future))
    }
    #[cfg(target_arch = "wasm32")]
    {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(output) => Some(output),
            Poll::Pending => None,
        }
    }
}

/// std's clocks on native; the browser's `performance.now()` and `Date.now()`
/// on the web, where `std::time::Instant::now()` panics.
pub mod time {
    pub use std::time::Duration;
    pub use web_time::{Instant, SystemTime, SystemTimeError, UNIX_EPOCH};

    /// The same moment as std's `SystemTime`, for libraries that take std's
    /// (chrono): the identity on native.
    pub fn to_std(t: SystemTime) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + t.duration_since(UNIX_EPOCH).unwrap_or_default()
    }

    /// std's `SystemTime` (from chrono) as this clock's: the identity on native.
    pub fn from_std(t: std::time::SystemTime) -> SystemTime {
        #[cfg(not(target_arch = "wasm32"))]
        {
            t
        }
        #[cfg(target_arch = "wasm32")]
        match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(after) => UNIX_EPOCH + after,
            Err(before) => UNIX_EPOCH - before.duration(),
        }
    }
}

/// std's threads. On the page, scoped threads run one after another on the
/// caller's and sleeping on the page's thread returns at once (it may not
/// block). Other threads start as Web Workers ([`crate::workers`]).
pub mod thread {
    pub use std::thread::*;

    #[cfg(target_arch = "wasm32")]
    pub use crate::workers::{Builder, JoinHandle, Thread, available_parallelism, sleep, spawn};
    #[cfg(target_arch = "wasm32")]
    pub use sequential::{Scope, ScopedJoinHandle, scope};

    #[cfg(target_arch = "wasm32")]
    mod sequential {
        use std::marker::PhantomData;

        pub struct Scope<'scope, 'env: 'scope> {
            _scope: PhantomData<&'scope mut &'scope ()>,
            _env: PhantomData<&'env mut &'env ()>,
        }

        pub struct ScopedJoinHandle<'scope, T> {
            result: T,
            _scope: PhantomData<&'scope ()>,
        }

        impl<'scope, 'env> Scope<'scope, 'env> {
            /// Runs `f` now and keeps what it returns for `join`.
            pub fn spawn<F, T>(&'scope self, f: F) -> ScopedJoinHandle<'scope, T>
            where
                F: FnOnce() -> T + Send + 'scope,
                T: Send + 'scope,
            {
                ScopedJoinHandle {
                    result: f(),
                    _scope: PhantomData,
                }
            }
        }

        impl<T> ScopedJoinHandle<'_, T> {
            pub fn join(self) -> std::thread::Result<T> {
                Ok(self.result)
            }
            pub fn is_finished(&self) -> bool {
                true
            }
        }

        pub fn scope<'env, F, T>(f: F) -> T
        where
            F: for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T,
        {
            f(&Scope {
                _scope: PhantomData,
                _env: PhantomData,
            })
        }
    }
}

/// std's timed waits that read std's clock, which panics on the web: there
/// they time themselves with the browser's. Never on the page's own thread,
/// which may not wait: it only checks once.
pub mod sync {
    use crate::time::Duration;
    use std::sync::{
        Condvar, LockResult, Mutex, MutexGuard, RwLock, RwLockWriteGuard,
        mpsc::{Receiver, RecvError, RecvTimeoutError},
    };

    /// `mutex.lock()`. The page's thread, which may not wait (the browser
    /// traps it), spins until the lock is free instead: use it for locks the
    /// page shares with workers, which hold them only briefly.
    pub fn lock<T: ?Sized>(mutex: &Mutex<T>) -> LockResult<MutexGuard<'_, T>> {
        #[cfg(target_arch = "wasm32")]
        if !can_wait() {
            loop {
                match mutex.try_lock() {
                    Ok(guard) => return Ok(guard),
                    Err(std::sync::TryLockError::Poisoned(e)) => return Err(e),
                    Err(std::sync::TryLockError::WouldBlock) => std::hint::spin_loop(),
                }
            }
        }
        mutex.lock()
    }

    /// `lock.write()`, spinning on the page's thread as [`lock`] does.
    pub fn write<T: ?Sized>(lock: &RwLock<T>) -> LockResult<RwLockWriteGuard<'_, T>> {
        #[cfg(target_arch = "wasm32")]
        if !can_wait() {
            loop {
                match lock.try_write() {
                    Ok(guard) => return Ok(guard),
                    Err(std::sync::TryLockError::Poisoned(e)) => return Err(e),
                    Err(std::sync::TryLockError::WouldBlock) => std::hint::spin_loop(),
                }
            }
        }
        lock.write()
    }

    /// `rx.recv()`.
    pub fn recv<T>(rx: &Receiver<T>) -> Result<T, RecvError> {
        rx.recv()
    }

    /// `rx.recv_timeout(timeout)`.
    pub fn recv_timeout<T>(rx: &Receiver<T>, timeout: Duration) -> Result<T, RecvTimeoutError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            rx.recv_timeout(timeout)
        }
        #[cfg(target_arch = "wasm32")]
        {
            use std::sync::mpsc::TryRecvError;
            let deadline = crate::time::Instant::now() + timeout;
            loop {
                match rx.try_recv() {
                    Ok(v) => return Ok(v),
                    Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
                    Err(TryRecvError::Empty) => {}
                }
                if !can_wait() || crate::time::Instant::now() >= deadline {
                    return Err(RecvTimeoutError::Timeout);
                }
                // A channel can't be waited on with a deadline here: look again shortly.
                crate::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// `rx.recv_timeout(timeout)` for a reply that a thread already running
    /// sends shortly. The page's thread, which may not wait, looks again
    /// until the deadline instead of giving up at once.
    pub fn recv_reply<T>(rx: &Receiver<T>, timeout: Duration) -> Result<T, RecvTimeoutError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            rx.recv_timeout(timeout)
        }
        #[cfg(target_arch = "wasm32")]
        {
            if can_wait() {
                return recv_timeout(rx, timeout);
            }
            use std::sync::mpsc::TryRecvError;
            let deadline = crate::time::Instant::now() + timeout;
            loop {
                match rx.try_recv() {
                    Ok(v) => return Ok(v),
                    Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
                    Err(TryRecvError::Empty) if crate::time::Instant::now() >= deadline => {
                        return Err(RecvTimeoutError::Timeout);
                    }
                    Err(TryRecvError::Empty) => std::hint::spin_loop(),
                }
            }
        }
    }

    /// `condvar.wait_while(guard, condition)` on `mutex`'s guard, without the
    /// poisoning.
    pub fn wait_while<'a, T, F>(
        mutex: &'a Mutex<T>,
        condvar: &Condvar,
        guard: MutexGuard<'a, T>,
        condition: F,
    ) -> MutexGuard<'a, T>
    where
        F: FnMut(&mut T) -> bool,
    {
        {
            let _ = mutex;
            condvar
                .wait_while(guard, condition)
                .unwrap_or_else(|e| e.into_inner())
        }
    }

    /// `condvar.wait_timeout_while(guard, timeout, condition)` on `mutex`'s
    /// guard, without the poisoning: the guard, and whether it timed out.
    pub fn wait_timeout_while<'a, T, F>(
        mutex: &'a Mutex<T>,
        condvar: &Condvar,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
        condition: F,
    ) -> (MutexGuard<'a, T>, bool)
    where
        F: FnMut(&mut T) -> bool,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = mutex;
            let (guard, result) = condvar
                .wait_timeout_while(guard, timeout, condition)
                .unwrap_or_else(|e| e.into_inner());
            (guard, result.timed_out())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (mut guard, mut condition) = (guard, condition);
            let deadline = crate::time::Instant::now() + timeout;
            while condition(&mut *guard) {
                let left = deadline.saturating_duration_since(crate::time::Instant::now());
                if !can_wait() || left.is_zero() {
                    return (guard, true);
                }
                let _ = mutex;
                // std's plain timed wait needs no clock.
                guard = condvar
                    .wait_timeout(guard, left)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            (guard, false)
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn can_wait() -> bool {
        crate::workers::in_worker()
    }
}

/// The process. The page has one game and no process id: 1.
pub mod process {
    #[cfg(not(target_arch = "wasm32"))]
    pub use std::process::id;

    #[cfg(target_arch = "wasm32")]
    pub fn id() -> u32 {
        1
    }
}
