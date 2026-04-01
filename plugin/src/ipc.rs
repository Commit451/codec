use crate::IpcParamOverrides;
use crossbeam_channel::{Sender, bounded};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

const LISTEN_ADDR: &str = "127.0.0.1:9847";

#[derive(Debug, Serialize)]
pub struct StateMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub cutoff: f32,
    pub resonance: f32,
}

#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub name: Option<String>,
    pub value: Option<f32>,
}

/// Runs in a background thread, managing the TCP server for UI connections.
pub struct IpcServer {
    /// Send state updates to the UI.
    pub state_tx: Sender<StateMessage>,
    running: Arc<AtomicBool>,
}

impl IpcServer {
    pub fn start(overrides: Arc<IpcParamOverrides>) -> Self {
        let (state_tx, state_rx) = bounded::<StateMessage>(64);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        thread::spawn(move || {
            Self::server_loop(overrides, state_rx, running_clone);
        });

        Self {
            state_tx,
            running,
        }
    }

    fn server_loop(
        overrides: Arc<IpcParamOverrides>,
        state_rx: crossbeam_channel::Receiver<StateMessage>,
        running: Arc<AtomicBool>,
    ) {
        let listener = match TcpListener::bind(LISTEN_ADDR) {
            Ok(l) => l,
            Err(e) => {
                nih_plug::prelude::nih_log!("IPC: failed to bind {LISTEN_ADDR}: {e}");
                return;
            }
        };

        listener
            .set_nonblocking(true)
            .expect("Cannot set non-blocking");

        nih_plug::prelude::nih_log!("IPC: listening on {LISTEN_ADDR}");

        while running.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, addr)) => {
                    nih_plug::prelude::nih_log!("IPC: client connected from {addr}");
                    stream.set_nonblocking(false).ok();
                    stream.set_nodelay(true).ok();
                    stream.set_read_timeout(None).ok();

                    let overrides = overrides.clone();
                    let running_r = running.clone();

                    // Clone stream for writing
                    let write_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    // Reader thread: parse incoming JSON and write to atomic overrides
                    thread::spawn(move || {
                        let reader = BufReader::new(&stream);
                        for line in reader.lines() {
                            if !running_r.load(Ordering::Relaxed) {
                                break;
                            }
                            let line = match line {
                                Ok(l) => l,
                                Err(e) => {
                                    if e.kind() == std::io::ErrorKind::WouldBlock
                                        || e.kind() == std::io::ErrorKind::TimedOut
                                    {
                                        continue;
                                    }
                                    break;
                                }
                            };
                            if line.is_empty() {
                                continue;
                            }
                            if let Ok(msg) = serde_json::from_str::<IncomingMessage>(&line) {
                                if msg.msg_type == "set_param" {
                                    if let (Some(name), Some(value)) = (msg.name, msg.value) {
                                        match name.as_str() {
                                            "cutoff" => overrides.set_cutoff(value),
                                            "resonance" => overrides.set_resonance(value),
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                        nih_plug::prelude::nih_log!("IPC: client disconnected");
                    });

                    // Writer: drain state_rx and send to client
                    let running_w = running.clone();
                    let state_rx_clone = state_rx.clone();
                    thread::spawn(move || {
                        let mut writer = std::io::BufWriter::new(write_stream);
                        while running_w.load(Ordering::Relaxed) {
                            match state_rx_clone.recv_timeout(std::time::Duration::from_millis(50)) {
                                Ok(msg) => {
                                    if let Ok(json) = serde_json::to_string(&msg) {
                                        let res = writeln!(writer, "{json}");
                                        let flush = writer.flush();
                                        if res.is_err() || flush.is_err() {
                                            break;
                                        }
                                    }
                                }
                                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                            }
                        }
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    nih_plug::prelude::nih_log!("IPC: accept error: {e}");
                    thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    }

    /// Send current parameter state to the UI.
    pub fn send_state(&self, cutoff: f32, resonance: f32) {
        let _ = self.state_tx.try_send(StateMessage {
            msg_type: "state".to_string(),
            cutoff,
            resonance,
        });
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
