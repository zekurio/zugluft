//! Pipe client to the zugluft service.
//!
//! One thread keeps the events connection alive and pumps state snapshots
//! into [`Shared`] (the UI polls it by sequence number). A second thread
//! forwards UI requests over the control pipe, (re)connecting on demand.
//! The two directions use separate pipe connections — see `zugluft_ipc`.

use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use zugluft_ipc::{self as ipc, Event, Request, ServiceState};

const RECONNECT_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Clone)]
pub enum UiState {
    /// Trying to reach the service pipe.
    Connecting,
    /// The pipe doesn't exist — service not installed or not running.
    ServiceUnavailable,
    Service(ServiceState),
}

pub struct Shared {
    seq: AtomicU64,
    state: Mutex<UiState>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            seq: AtomicU64::new(0),
            state: Mutex::new(UiState::Connecting),
        }
    }
}

impl Shared {
    fn state_guard(&self) -> MutexGuard<'_, UiState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Acquire)
    }

    pub fn state(&self) -> UiState {
        self.state_guard().clone()
    }

    fn publish(&self, state: UiState) {
        *self.state_guard() = state;
        self.seq.fetch_add(1, Ordering::Release);
    }
}

pub fn spawn(shared: Arc<Shared>) -> Sender<Request> {
    let (tx, rx) = channel::<Request>();

    // Request forwarder: control connection, established on demand and
    // re-established once per failed send.
    thread::Builder::new()
        .name("zugluft-control".into())
        .spawn(move || {
            let mut control: Option<File> = None;
            while let Ok(request) = rx.recv() {
                if control.is_none() {
                    control = ipc::pipe::connect_control().ok();
                }
                let sent = control
                    .as_mut()
                    .is_some_and(|stream| ipc::send(stream, &request).is_ok());
                if !sent {
                    control = ipc::pipe::connect_control().ok();
                    if let Some(stream) = control.as_mut()
                        && ipc::send(stream, &request).is_err()
                    {
                        control = None;
                    }
                }
            }
        })
        .expect("failed to spawn control thread");

    // Event stream: connect, pump states, reconnect on loss.
    thread::Builder::new()
        .name("zugluft-events".into())
        .spawn(move || {
            loop {
                let stream = match ipc::pipe::connect_events() {
                    Ok(stream) => stream,
                    Err(_) => {
                        shared.publish(UiState::ServiceUnavailable);
                        thread::sleep(RECONNECT_INTERVAL);
                        continue;
                    }
                };
                shared.publish(UiState::Connecting); // until the first event lands

                let mut reader = BufReader::new(stream);
                while let Ok(Some(Event::State(state))) = ipc::recv::<Event>(&mut reader) {
                    shared.publish(UiState::Service(state));
                }

                shared.publish(UiState::ServiceUnavailable);
                thread::sleep(RECONNECT_INTERVAL);
            }
        })
        .expect("failed to spawn events thread");

    tx
}
