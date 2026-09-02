// SPDX-License-Identifier: Apache-2.0

//! Non-blocking telemetry publication for the UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::backend::TelemetryBackend;
use crate::backend::nvml::{NvmlBackend, NvmlConfig};
use crate::model::Snapshot;

pub struct Collector {
    latest: Arc<RwLock<Option<Snapshot>>>,
    last_error: Arc<RwLock<Option<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Collector {
    pub fn spawn(config: NvmlConfig, interval: Duration) -> anyhow::Result<Self> {
        let latest = Arc::new(RwLock::new(None));
        let last_error = Arc::new(RwLock::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let latest_thread = Arc::clone(&latest);
        let error_thread = Arc::clone(&last_error);
        let stop_thread = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("nvml-collector".to_owned())
            .spawn(move || {
                let mut backend = match NvmlBackend::new(config) {
                    Ok(backend) => backend,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                };

                match backend.sample() {
                    Ok(snapshot) => {
                        if let Ok(mut slot) = latest_thread.write() {
                            *slot = Some(snapshot);
                        }
                        let _ = ready_tx.send(Ok(()));
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                }

                while !stop_thread.load(Ordering::Relaxed) {
                    thread::park_timeout(interval);
                    if stop_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    match backend.sample() {
                        Ok(snapshot) => {
                            if let Ok(mut slot) = latest_thread.write() {
                                *slot = Some(snapshot);
                            }
                            if let Ok(mut error) = error_thread.write() {
                                *error = None;
                            }
                        }
                        Err(collection_error) => {
                            if let Ok(mut error) = error_thread.write() {
                                *error = Some(collection_error.to_string());
                            }
                        }
                    }
                }
            })?;

        ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| anyhow::anyhow!("collector initialization timed out: {error}"))?
            .map_err(anyhow::Error::msg)?;

        Ok(Self {
            latest,
            last_error,
            stop,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.latest.read().ok()?.clone()
    }

    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        self.last_error.read().ok()?.clone()
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}
