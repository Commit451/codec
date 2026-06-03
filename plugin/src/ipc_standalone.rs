//! Simplified IPC server for standalone mode.
//! Same JSON protocol as the plugin IPC, callback-based API.
//! Incoming `set_param` messages invoke `on_param(name, value)`; outgoing state
//! is sent as pre-serialized JSON lines (the caller owns the schema).

use crossbeam_channel::{bounded, Sender};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const LISTEN_ADDR: &str = "127.0.0.1:9847";

#[derive(Debug, Deserialize)]
struct IncomingMessage {
    #[serde(rename = "type")]
    msg_type: String,
    name: Option<String>,
    value: Option<f32>,
}

/// Start the IPC server for standalone mode.
/// `on_param` is called whenever the UI sends a parameter change.
/// Returns a sender for pre-serialized state JSON lines and the server thread handle.
pub fn start_standalone_ipc<F>(on_param: F) -> (Sender<String>, JoinHandle<()>)
where
    F: Fn(&str, f32) + Send + Sync + 'static,
{
    let (state_tx, state_rx) = bounded::<String>(64);
    let running = Arc::new(AtomicBool::new(true));
    let on_param = Arc::new(on_param);

    let handle = thread::spawn(move || {
        let listener = match TcpListener::bind(LISTEN_ADDR) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("IPC: failed to bind {LISTEN_ADDR}: {e}");
                return;
            }
        };

        listener.set_nonblocking(true).expect("Cannot set non-blocking");
        println!("IPC: listening on {LISTEN_ADDR}");

        while running.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, addr)) => {
                    println!("IPC: UI connected from {addr}");

                    let read_stream = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let write_stream = stream;

                    let on_param_clone = on_param.clone();
                    let running_r = running.clone();
                    thread::spawn(move || {
                        read_loop(read_stream, &*on_param_clone, &running_r);
                        println!("IPC: UI disconnected");
                    });

                    let state_rx_clone = state_rx.clone();
                    let running_w = running.clone();
                    thread::spawn(move || {
                        write_loop(write_stream, state_rx_clone, &running_w);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("IPC: accept error: {e}");
                    thread::sleep(std::time::Duration::from_millis(500));
                }
            }
        }
    });

    (state_tx, handle)
}

fn read_loop<F: Fn(&str, f32)>(stream: TcpStream, on_param: &F, running: &AtomicBool) {
    stream.set_nonblocking(false).ok();
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(None).ok();

    let reader = BufReader::new(stream);
    for line in reader.lines() {
        if !running.load(Ordering::Relaxed) {
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
                if let (Some(ref name), Some(value)) = (msg.name, msg.value) {
                    on_param(name, value);
                }
            }
        }
    }
}

fn write_loop(stream: TcpStream, state_rx: crossbeam_channel::Receiver<String>, running: &AtomicBool) {
    let mut writer = std::io::BufWriter::new(stream);
    while running.load(Ordering::Relaxed) {
        match state_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(line) => {
                if writeln!(writer, "{line}").is_err() || writer.flush().is_err() {
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}
