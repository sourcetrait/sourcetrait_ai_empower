use crate::*;
use std::time::Instant;

/// What: a bounded pool of `WorkerHandle`s for the stateless `run` /
/// `rerun` / `call` substrate. Capacity is fixed at construction;
/// workers spawn lazily on first acquire (up to cap), get returned to
/// a free list on release, and idle-reap back toward a minimum after
/// `idle_timeout`. Acquire blocks when all workers are in flight and
/// the cap is reached.
///
/// Why: slice 5.8 replaces the single-`runs_worker` AsyncMutex with
/// this pool so concurrent agent `run()` calls execute in parallel on
/// distinct workers. Killing one worker (slice 5.9 cancel or slice 5.10
/// timeout) no longer blocks queued calls -- they take other workers
/// out of the pool. Interact stays single-worker because stateful
/// sessions can't be sensibly pooled.
///
/// Where: instantiated once in `server::run::run_server` for the runs
/// pool; held as `runs_pool: Arc<Pool>` on `NuSh`. Each dispatching
/// handler (`run`, `rerun`, `call`) acquires through it.
pub(crate) struct Pool {
    #[allow(dead_code)]
    cap: usize,
    min: usize,
    idle_timeout: tk::TkDuration,
    mode: Mode,
    semaphore: Arc<tk::Semaphore>,
    free: Arc<tk::AsyncMutex<Vec<FreeSlot>>>,
    in_use: Arc<AtomicUsize>,
}

struct FreeSlot {
    handle: WorkerHandle,
    released_at: Instant,
}

impl Pool {
    /// What: build a new `Pool` and spawn its idle-reaper background
    /// task. Returns an `Arc<Self>` so the pool can be shared across
    /// handlers and across the reaper task without lifetime juggling.
    ///
    /// Why: spawning the reaper inside `new` keeps the pool's lifecycle
    /// self-contained. The reaper holds a `Weak<Self>` so it stops
    /// automatically when the pool's last `Arc` drops.
    ///
    /// Where: called once in `server::run::run_server` with `mode =
    /// Stateless`, the runtime-detected cap, `min = 1`, and a 60s
    /// idle timeout.
    pub(crate) fn new(
        mode: Mode,
        cap: usize,
        min: usize,
        idle_timeout: tk::TkDuration,
    ) -> Arc<Self> {
        let pool = Arc::new(Self {
            cap,
            min,
            idle_timeout,
            mode,
            semaphore: Arc::new(tk::Semaphore::new(cap)),
            free: Arc::new(tk::AsyncMutex::new(Vec::new())),
            in_use: Arc::new(AtomicUsize::new(0)),
        });
        let weak = Arc::downgrade(&pool);
        tk::spawn(async move {
            reaper_loop(weak).await;
        });
        pool
    }

    /// What: acquire a `PooledGuard`. Pops a warm worker from the free
    /// list when available; otherwise spawns a fresh one (which runs
    /// `WarmBase::new` including plugin load + `generate_nu_constant`).
    /// Blocks via the inner semaphore when all `cap` workers are in
    /// flight, until one is released.
    ///
    /// Why: lazy spawn keeps idle-state resource use minimal; the
    /// semaphore enforces the cap; the free list reuses already-warmed
    /// workers (saves plugin load + setsid + env seeding).
    ///
    /// Where: called by `server::tool::dispatch_to_worker` for every
    /// stateless tool call (run / rerun / call).
    pub(crate) async fn acquire(self: &Arc<Self>) -> io::Result<PooledGuard> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| io::Error::other(format!("pool semaphore: {e}")))?;
        let handle_opt = {
            let mut free = self.free.lock().await;
            free.pop()
        };
        let handle = match handle_opt {
            Some(slot) => slot.handle,
            None => WorkerHandle::spawn(self.mode).await?,
        };
        self.in_use.fetch_add(1, Ordering::SeqCst);
        Ok(PooledGuard {
            pool: self.clone(),
            handle: Some(handle),
            _permit: permit,
        })
    }

    /// What: push `handle` back onto the free list and decrement
    /// `in_use`. Called by `PooledGuard::drop` via a spawned task
    /// because Drop can't await.
    async fn release(self: &Arc<Self>, handle: WorkerHandle) {
        let mut free = self.free.lock().await;
        free.push(FreeSlot {
            handle,
            released_at: Instant::now(),
        });
        self.in_use.fetch_sub(1, Ordering::SeqCst);
    }

    /// Mark that a previously-acquired worker was killed (not returned
    /// to the free list). Decrements `in_use` so reaper math stays
    /// honest.
    fn killed(&self) {
        self.in_use.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What: RAII guard around a checked-out worker. Holds a semaphore
/// permit + the `WorkerHandle`. Dispatches via `send_request`. On Drop,
/// returns the handle to the pool's free list (unless `drop_handle`
/// was called, e.g. when the worker was killed).
///
/// Why: ties worker check-out to a value the dispatch function holds;
/// when the function returns / panics / cancels, the guard's Drop
/// either restores the worker or accounts for its death.
///
/// Where: returned from `Pool::acquire`; held by
/// `server::tool::dispatch_to_worker` for the duration of one tool
/// call.
pub(crate) struct PooledGuard {
    pool: Arc<Pool>,
    handle: Option<WorkerHandle>,
    _permit: tk::OwnedSemaphorePermit,
}

impl PooledGuard {
    pub(crate) async fn send_request(
        &mut self,
        log_dir: PathBuf,
        source: String,
    ) -> io::Result<RunResponse> {
        self.handle
            .as_mut()
            .expect("PooledGuard.handle present")
            .send_request(log_dir, source)
            .await
    }

    pub(crate) fn pid(&self) -> u32 {
        self.handle
            .as_ref()
            .expect("PooledGuard.handle present")
            .pid()
    }

    /// Drop the worker without returning it to the pool. Used after
    /// a kill or timeout when the worker process is already dead;
    /// returning it would queue a dead handle.
    pub(crate) fn drop_handle(&mut self) {
        if self.handle.take().is_some() {
            self.pool.killed();
        }
    }
}

impl Drop for PooledGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let pool = self.pool.clone();
            tk::spawn(async move {
                pool.release(handle).await;
            });
        }
    }
}

/// What: background task that periodically reaps free-list workers
/// idle longer than `pool.idle_timeout`, keeping at least `pool.min`
/// total workers alive (free + in_use).
///
/// Why: pool growth handles burst load; reaping returns idle resources
/// (process slots, plugin children of those workers) once the burst
/// passes. Keeping `min` warm avoids cold-start on the next call after
/// idle.
///
/// Where: spawned once per `Pool::new`. Stops when the pool's last
/// `Arc` drops (the weak ref fails to upgrade).
async fn reaper_loop(weak: std::sync::Weak<Pool>) {
    let mut ticker = tk::interval(tk::TkDuration::from_secs(15));
    ticker.tick().await; // discard immediate first tick
    loop {
        ticker.tick().await;
        let Some(pool) = weak.upgrade() else {
            return;
        };
        let now = Instant::now();
        let mut free = pool.free.lock().await;
        let in_use = pool.in_use.load(Ordering::SeqCst);
        // Reap from the FRONT (oldest released first); acquire pops
        // from the back so newest stays warm. Stop when reaping one
        // more would drop alive count below `min`.
        let mut to_drop = 0usize;
        for slot in free.iter() {
            let alive_after = free.len() - (to_drop + 1) + in_use;
            if alive_after < pool.min {
                break;
            }
            if now.duration_since(slot.released_at) >= pool.idle_timeout {
                to_drop += 1;
            } else {
                break;
            }
        }
        if to_drop > 0 {
            // Drop drains; SIGKILL via WorkerHandle's Drop impl.
            let _ = free.drain(0..to_drop).collect::<Vec<_>>();
        }
    }
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Pool>();
    assert_send::<PooledGuard>();
};
