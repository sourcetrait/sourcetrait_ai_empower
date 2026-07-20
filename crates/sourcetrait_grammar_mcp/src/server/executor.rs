use crate::*;

/// The stateless eval executor: a swappable read-only base engine plus a small
/// buffer of pre-built clones, so a launching run/call takes a ready clone
/// instead of paying the clone inline. Concurrency is semaphore-bounded
/// (`eval_concurrency_cap`); the buffer is a latency pre-preparation, NOT clone
/// reuse - each clone is single-use (dropped after its one eval, statelessness
/// intact).
///
/// Correctness hinge (base invalidation): a pre-cloned engine is a SNAPSHOT of
/// the base's plugin decls + env. The base is a swap point tagged with a
/// generation; a plugin-registry change (a `plugin add/rm` on the interact lane
/// OR an external edit, both seen via the file mtime) rebuilds the base, bumps
/// the generation, and drops the now-stale ready buffer - so run() picks up a
/// plugin the interact lane just registered, without a restart. In-flight evals
/// finish on the clone they already hold (dispatched under the old state - fine);
/// only the ready buffer + future clones pick up the change.
pub(crate) struct Executor {
    base: std::sync::Mutex<BaseHolder>,
    ready: std::sync::Mutex<Vec<ReadyClone>>,
    env_jobs: Arc<std::sync::Mutex<nu::Jobs>>,
    semaphore: Arc<tk::Semaphore>,
    ready_target: usize,
}

struct BaseHolder {
    engine: Arc<nu::EngineState>,
    generation: u64,
    registry_mtime: Option<SystemTime>,
}

struct ReadyClone {
    engine: nu::EngineState,
    generation: u64,
}

impl Executor {
    /// Build the base once, snapshot the registry mtime, pre-fill `ready_target`
    /// clones. `ready_target` is the pre-clone depth N (default 1).
    pub(crate) fn new(env_jobs: Arc<std::sync::Mutex<nu::Jobs>>, ready_target: usize) -> Self {
        let holder = BaseHolder {
            engine: Arc::new(build_base(Mode::Stateless)),
            generation: 0,
            registry_mtime: registry_mtime(),
        };
        let executor = Self {
            base: std::sync::Mutex::new(holder),
            ready: std::sync::Mutex::new(Vec::new()),
            env_jobs,
            semaphore: Arc::new(tk::Semaphore::new(eval_concurrency_cap())),
            ready_target,
        };
        executor.refill_ready();
        executor
    }

    /// The concurrency gate: acquire a permit before launching an eval, so at
    /// most `eval_concurrency_cap()` evals run at once. The permit rides in the
    /// eval thread and releases when it finishes (a hung thread holds it - the residual).
    pub(crate) fn semaphore(&self) -> Arc<tk::Semaphore> {
        self.semaphore.clone()
    }

    /// Top the ready buffer up to `ready_target` with current-generation clones.
    /// The clone (the cost) happens outside the ready lock.
    fn refill_ready(&self) {
        loop {
            let need = {
                let ready = self.ready.lock().expect("executor ready lock");
                self.ready_target.saturating_sub(ready.len())
            };
            if need == 0 {
                break;
            }
            let (engine, generation) = {
                let holder = self.base.lock().expect("executor base lock");
                let mut engine = (*holder.engine).clone();
                engine.jobs = self.env_jobs.clone();
                (engine, holder.generation)
            };
            let mut ready = self.ready.lock().expect("executor ready lock");
            if ready.len() < self.ready_target {
                ready.push(ReadyClone { engine, generation });
            } else {
                break;
            }
        }
    }

    /// Stat the plugin registry; if it changed since the base was built, rebuild
    /// the base (new generation) and drop the now-stale ready buffer. Cheap
    /// (~sub-microsecond stat) and only rebuilds on an actual change.
    pub(crate) fn refresh_base_if_stale(&self) {
        let current = registry_mtime();
        {
            let mut holder = self.base.lock().expect("executor base lock");
            if current == holder.registry_mtime {
                return;
            }
            holder.engine = Arc::new(build_base(Mode::Stateless));
            holder.generation += 1;
            holder.registry_mtime = current;
        }
        self.ready.lock().expect("executor ready lock").clear();
        self.refill_ready();
    }

    /// Take a ready clone (or clone fresh if the buffer is empty / stale), then
    /// refill the buffer for the next caller. The returned engine carries the
    /// shared env_jobs and is ready to eval. Holds the base lock across the pop
    /// so the generation read and the fallback clone stay consistent with a
    /// concurrent `refresh_base_if_stale`.
    pub(crate) fn take_clone(&self) -> nu::EngineState {
        let engine = {
            let holder = self.base.lock().expect("executor base lock");
            let generation = holder.generation;
            let mut ready = self.ready.lock().expect("executor ready lock");
            ready.retain(|rc| rc.generation == generation);
            match ready.pop() {
                Some(rc) => rc.engine,
                None => {
                    let mut engine = (*holder.engine).clone();
                    engine.jobs = self.env_jobs.clone();
                    engine
                }
            }
        };
        self.refill_ready();
        engine
    }
}
