//! Named-pipe servers: the events pipe streams state to clients, the
//! control pipe receives their requests. Each connection is strictly
//! one-directional (see `zugluft_ipc` for why).

use std::io::BufReader;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use zugluft_ipc::{self as ipc, pipe::PipeServer};

use crate::hub::Hub;
use crate::log_line;
use crate::worker::Command;

/// Spawns the accept loops for both pipes.
pub fn spawn_listeners(hub: Arc<Hub>, worker: Sender<Command>) {
    thread::Builder::new()
        .name("zugluft-events".into())
        .spawn(move || {
            let mut server = PipeServer::new(ipc::EVENTS_PIPE);
            loop {
                match server.accept() {
                    Ok(stream) => {
                        let events = hub.subscribe();
                        thread::Builder::new()
                            .name("zugluft-events-client".into())
                            .spawn(move || {
                                let mut stream = stream;
                                // Write-only: forward events until the pipe
                                // breaks; the broken send also prunes the hub
                                // subscription on the next publish.
                                for event in events {
                                    if ipc::send(&mut stream, &event).is_err() {
                                        break;
                                    }
                                }
                            })
                            .ok();
                    }
                    Err(error) => {
                        log_line(&format!("events pipe accept failed: {error}"));
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        })
        .expect("failed to spawn events listener");

    thread::Builder::new()
        .name("zugluft-control".into())
        .spawn(move || {
            let mut server = PipeServer::new(ipc::CONTROL_PIPE);
            loop {
                match server.accept() {
                    Ok(stream) => {
                        let worker = worker.clone();
                        thread::Builder::new()
                            .name("zugluft-control-client".into())
                            .spawn(move || {
                                // Read-only: forward requests until disconnect.
                                let mut reader = BufReader::new(stream);
                                while let Ok(Some(request)) = ipc::recv::<ipc::Request>(&mut reader)
                                {
                                    if worker.send(Command::Request(request)).is_err() {
                                        break;
                                    }
                                }
                            })
                            .ok();
                    }
                    Err(error) => {
                        log_line(&format!("control pipe accept failed: {error}"));
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        })
        .expect("failed to spawn control listener");
}
