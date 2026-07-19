use crate::*;
use std::time::Instant;

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
            permit: Some(permit),
        })
    }

    async fn release(self: &Arc<Self>, handle: WorkerHandle) {
        let mut free = self.free.lock().await;
        free.push(FreeSlot {
            handle,
            released_at: Instant::now(),
        });
        self.in_use.fetch_sub(1, Ordering::SeqCst);
    }

    fn killed(&self) {
        self.in_use.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) struct PooledGuard {
    pool: Arc<Pool>,
    handle: Option<WorkerHandle>,
    permit: Option<tk::OwnedSemaphorePermit>,
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
            let permit = self.permit.take();
            tk::spawn(async move {
                pool.release(handle).await;
                drop(permit);
            });
        }
    }
}

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
            let _ = free.drain(0..to_drop).collect::<Vec<_>>();
        }
    }
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Pool>();
    assert_send::<PooledGuard>();
};
