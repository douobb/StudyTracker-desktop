use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::error::{AppError, AppResult};

pub trait ManagedResource: Send + Sync {
    fn start(&self) -> AppResult<()>;
    fn stop(&self) -> AppResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Running,
    Stopped,
}

pub struct RuntimeCoordinator {
    resources: Vec<Arc<dyn ManagedResource>>,
    state: Mutex<RuntimeState>,
}

impl RuntimeCoordinator {
    pub fn new(resources: Vec<Arc<dyn ManagedResource>>) -> Self {
        Self {
            resources,
            state: Mutex::new(RuntimeState::Stopped),
        }
    }

    pub fn start(&self) -> AppResult<()> {
        let mut state = self.state.lock().map_err(|_| AppError::Runtime)?;
        if *state == RuntimeState::Running {
            return Err(AppError::RuntimeAlreadyRunning);
        }
        let mut started: Vec<&Arc<dyn ManagedResource>> = Vec::new();
        for resource in &self.resources {
            if resource.start().is_err() {
                for previous in started.into_iter().rev() {
                    let _ = previous.stop();
                }
                return Err(AppError::Runtime);
            }
            started.push(resource);
        }
        *state = RuntimeState::Running;
        Ok(())
    }

    pub fn stop(&self) -> AppResult<()> {
        let mut state = self.state.lock().map_err(|_| AppError::Runtime)?;
        if *state == RuntimeState::Stopped {
            return Ok(());
        }
        let mut failed = false;
        for resource in self.resources.iter().rev() {
            failed |= resource.stop().is_err();
        }
        *state = RuntimeState::Stopped;
        if failed {
            Err(AppError::Runtime)
        } else {
            Ok(())
        }
    }

    pub fn state(&self) -> AppResult<RuntimeState> {
        self.state
            .lock()
            .map(|state| *state)
            .map_err(|_| AppError::Runtime)
    }
}

impl Drop for RuntimeCoordinator {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Default)]
pub struct HeartbeatResource {
    worker: Mutex<Option<HeartbeatWorker>>,
}

struct HeartbeatWorker {
    cancel: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl ManagedResource for HeartbeatResource {
    fn start(&self) -> AppResult<()> {
        let mut worker = self.worker.lock().map_err(|_| AppError::Runtime)?;
        if worker.is_some() {
            return Err(AppError::RuntimeAlreadyRunning);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = cancel.clone();
        let handle = thread::Builder::new()
            .name("studytracker-heartbeat".to_string())
            .spawn(move || {
                while !thread_cancel.load(Ordering::Acquire) {
                    thread::park_timeout(Duration::from_millis(250));
                }
            })
            .map_err(|_| AppError::Runtime)?;
        *worker = Some(HeartbeatWorker { cancel, handle });
        Ok(())
    }

    fn stop(&self) -> AppResult<()> {
        let mut worker = self.worker.lock().map_err(|_| AppError::Runtime)?;
        if let Some(worker) = worker.take() {
            worker.cancel.store(true, Ordering::Release);
            worker.handle.thread().unpark();
            worker.handle.join().map_err(|_| AppError::Runtime)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::{ManagedResource, RuntimeCoordinator, RuntimeState};
    use crate::error::AppResult;

    #[derive(Default)]
    struct CountingResource {
        starts: AtomicUsize,
        stops: AtomicUsize,
    }

    impl ManagedResource for CountingResource {
        fn start(&self) -> AppResult<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn stop(&self) -> AppResult<()> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn 不會重複啟動且停止會封口所有資源() {
        let observer = Arc::new(CountingResource::default());
        let interval_writer = Arc::new(CountingResource::default());
        let coordinator = RuntimeCoordinator::new(vec![observer.clone(), interval_writer.clone()]);

        coordinator.start().unwrap();
        assert!(coordinator.start().is_err());
        coordinator.stop().unwrap();
        coordinator.stop().unwrap();

        assert_eq!(observer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(observer.stops.load(Ordering::SeqCst), 1);
        assert_eq!(interval_writer.starts.load(Ordering::SeqCst), 1);
        assert_eq!(interval_writer.stops.load(Ordering::SeqCst), 1);
        assert_eq!(coordinator.state().unwrap(), RuntimeState::Stopped);
    }
}
