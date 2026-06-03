use crate::{CodecParams, ParamEdit, ParamId, Shared};
use crossbeam_channel::{bounded, Sender};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

const LISTEN_ADDR: &str = "127.0.0.1:9847";

/// All parameter values plus meters, sent to the UI ~30 Hz.
#[derive(Debug, Serialize)]
pub struct StateMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub density: f32,
    pub size: f32,
    pub position: f32,
    pub spray: f32,
    pub pitch: f32,
    pub pitch_spread: f32,
    pub pan_spread: f32,
    pub feedback: f32,
    pub mix: f32,
    pub sync: i32,
    pub reverse: i32,
    pub division: i32,
    pub bpm: f32,
    pub level: f32,
    pub grains: i32,
}

#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub name: Option<String>,
    pub value: Option<f32>,
    /// For `"gesture"` messages: `"begin"` or `"end"`.
    pub action: Option<String>,
}

fn param_id(name: &str) -> Option<ParamId> {
    match name {
        "density" => Some(ParamId::Density),
        "size" => Some(ParamId::Size),
        "position" => Some(ParamId::Position),
        "spray" => Some(ParamId::Spray),
        "pitch" => Some(ParamId::Pitch),
        "pitch_spread" => Some(ParamId::PitchSpread),
        "pan_spread" => Some(ParamId::PanSpread),
        "feedback" => Some(ParamId::Feedback),
        "mix" => Some(ParamId::Mix),
        "sync" => Some(ParamId::Sync),
        "reverse" => Some(ParamId::Reverse),
        "division" => Some(ParamId::Division),
        _ => None,
    }
}

/// Runs in a background thread, managing the TCP server for UI connections.
pub struct IpcServer {
    pub state_tx: Sender<StateMessage>,
    shared: Arc<Shared>,
    running: Arc<AtomicBool>,
}

impl IpcServer {
    pub fn start(shared: Arc<Shared>) -> Self {
        let (state_tx, state_rx) = bounded::<StateMessage>(64);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let shared_clone = shared.clone();

        thread::spawn(move || {
            Self::server_loop(shared_clone, state_rx, running_clone);
        });

        Self {
            state_tx,
            shared,
            running,
        }
    }

    fn server_loop(
        shared: Arc<Shared>,
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

                    shared.ui_connected.store(true, Ordering::Relaxed);

                    let shared_r = shared.clone();
                    let running_r = running.clone();

                    let write_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    // Reader thread: parse incoming JSON and forward parameter edits.
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
                                handle_message(&shared_r, &msg);
                            }
                        }
                        // Connection closed: drop the connected flag and release any
                        // gestures left open by an in-flight drag.
                        shared_r.ui_connected.store(false, Ordering::Relaxed);
                        release_all_gestures(&shared_r);
                        nih_plug::prelude::nih_log!("IPC: client disconnected");
                    });

                    // Writer: drain state_rx and send to client.
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

    /// Send the current parameter state + meters to the UI.
    pub fn send_state(&self, params: &CodecParams, bpm: f32, level: f32) {
        let _ = self.state_tx.try_send(StateMessage {
            msg_type: "state",
            density: params.density.value(),
            size: params.size.value(),
            position: params.position.value(),
            spray: params.spray.value(),
            pitch: params.pitch.value(),
            pitch_spread: params.pitch_spread.value(),
            pan_spread: params.pan_spread.value(),
            feedback: params.feedback.value(),
            mix: params.mix.value(),
            sync: params.sync.value() as i32,
            reverse: params.reverse.value() as i32,
            division: params.division.value().index(),
            bpm,
            level,
            grains: self.shared.active_grains.load(Ordering::Relaxed) as i32,
        });
    }
}

/// Translate an incoming IPC message into a queued [`ParamEdit`].
fn handle_message(shared: &Shared, msg: &IncomingMessage) {
    match msg.msg_type.as_str() {
        "set_param" => {
            if let (Some(name), Some(value)) = (msg.name.as_deref(), msg.value) {
                if let Some(id) = param_id(name) {
                    let _ = shared.edit_tx.try_send(ParamEdit::Set(id, value));
                }
            }
        }
        "gesture" => {
            if let (Some(name), Some(action)) = (msg.name.as_deref(), msg.action.as_deref()) {
                if let Some(id) = param_id(name) {
                    let edit = match action {
                        "begin" => Some(ParamEdit::Begin(id)),
                        "end" => Some(ParamEdit::End(id)),
                        _ => None,
                    };
                    if let Some(edit) = edit {
                        let _ = shared.edit_tx.try_send(edit);
                    }
                }
            }
        }
        _ => {}
    }
}

fn release_all_gestures(shared: &Shared) {
    for id in [
        ParamId::Density,
        ParamId::Size,
        ParamId::Position,
        ParamId::Spray,
        ParamId::Pitch,
        ParamId::PitchSpread,
        ParamId::PanSpread,
        ParamId::Feedback,
        ParamId::Mix,
        ParamId::Sync,
        ParamId::Reverse,
        ParamId::Division,
    ] {
        let _ = shared.edit_tx.try_send(ParamEdit::End(id));
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.shared.ui_connected.store(false, Ordering::Relaxed);
    }
}
