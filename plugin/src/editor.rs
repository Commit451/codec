//! Minimal in-host editor (egui).
//!
//! This window exists for two reasons:
//!  1. It's a small, fully functional in-host UI so the plugin behaves like a
//!     normal automatable plugin even if the Compose Desktop UI is never launched.
//!  2. It's the *bridge* that lets the external Compose UI participate in the host
//!     parameter system. nih-plug only lets you write a parameter (so the host can
//!     record automation and persist it) through a `ParamSetter`, which is only
//!     available from an editor running on the GUI thread. The Compose UI is a
//!     separate process and has no such handle, so its edits arrive over IPC, get
//!     queued as [`ParamEdit`]s, and are applied here on the GUI thread.

use crate::{ComposeVstParams, ParamEdit, ParamId, Shared};
use crossbeam_channel::Receiver;
use nih_plug::prelude::{Editor, ParamSetter};
use nih_plug_egui::{create_egui_editor, egui, widgets};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// State owned by the editor and mutated only on the GUI thread.
struct EditorState {
    params: Arc<ComposeVstParams>,
    shared: Arc<Shared>,
    edit_rx: Receiver<ParamEdit>,
    /// Whether an automation gesture is currently open for each param, indexed by
    /// `ParamId as usize`. Used to keep begin/set/end well-formed even if an IPC
    /// message is dropped.
    in_gesture: [bool; 3],
}

pub(crate) fn create(
    params: Arc<ComposeVstParams>,
    shared: Arc<Shared>,
    edit_rx: Receiver<ParamEdit>,
) -> Option<Box<dyn Editor>> {
    let egui_state = params.editor_state.clone();
    create_egui_editor(
        egui_state,
        EditorState {
            params,
            shared,
            edit_rx,
            in_gesture: [false; 3],
        },
        |_, _| {},
        move |egui_ctx, setter, state| {
            // Apply edits forwarded from the external Compose UI before drawing, so
            // host automation recording sees them on this (GUI) thread.
            drain_edits(state, setter);

            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.heading("Compose VST");

                let bpm = f32::from_bits(state.shared.bpm.load(Ordering::Relaxed));
                ui.label(if bpm > 0.0 {
                    format!("Host tempo: {bpm:.1} BPM")
                } else {
                    "Host tempo: — (no transport)".to_string()
                });
                ui.label(if state.shared.ui_connected.load(Ordering::Relaxed) {
                    "Compose UI: connected"
                } else {
                    "Compose UI: not connected"
                });

                ui.separator();

                ui.label("Cutoff");
                ui.add(widgets::ParamSlider::for_param(&state.params.cutoff, setter));
                ui.label("Resonance");
                ui.add(widgets::ParamSlider::for_param(
                    &state.params.resonance,
                    setter,
                ));
                ui.label("Tempo Sweep (bar-synced)");
                ui.add(widgets::ParamSlider::for_param(
                    &state.params.sweep_depth,
                    setter,
                ));

                ui.separator();
                ui.small("Optional richer UI: cd ui && ./gradlew run");
                ui.small("Keep this window open to record Compose edits as automation.");
            });

            // egui only repaints on input by default; request continuous repaints so
            // the IPC edit queue keeps draining while a Compose drag is in flight.
            egui_ctx.request_repaint();
        },
    )
}

fn drain_edits(state: &mut EditorState, setter: &ParamSetter) {
    while let Ok(edit) = state.edit_rx.try_recv() {
        match edit {
            ParamEdit::Begin(id) => begin(state, setter, id),
            ParamEdit::Set(id, value) => {
                // Open a gesture defensively in case the begin message was dropped,
                // so the host always sees begin -> set -> end.
                begin(state, setter, id);
                match id {
                    ParamId::Cutoff => setter.set_parameter(&state.params.cutoff, value),
                    ParamId::Resonance => setter.set_parameter(&state.params.resonance, value),
                    ParamId::SweepDepth => setter.set_parameter(&state.params.sweep_depth, value),
                }
            }
            ParamEdit::End(id) => end(state, setter, id),
        }
    }
}

fn begin(state: &mut EditorState, setter: &ParamSetter, id: ParamId) {
    if state.in_gesture[id as usize] {
        return;
    }
    state.in_gesture[id as usize] = true;
    match id {
        ParamId::Cutoff => setter.begin_set_parameter(&state.params.cutoff),
        ParamId::Resonance => setter.begin_set_parameter(&state.params.resonance),
        ParamId::SweepDepth => setter.begin_set_parameter(&state.params.sweep_depth),
    }
}

fn end(state: &mut EditorState, setter: &ParamSetter, id: ParamId) {
    if !state.in_gesture[id as usize] {
        return;
    }
    state.in_gesture[id as usize] = false;
    match id {
        ParamId::Cutoff => setter.end_set_parameter(&state.params.cutoff),
        ParamId::Resonance => setter.end_set_parameter(&state.params.resonance),
        ParamId::SweepDepth => setter.end_set_parameter(&state.params.sweep_depth),
    }
}
