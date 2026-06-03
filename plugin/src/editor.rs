//! Minimal in-host editor (egui).
//!
//! Two jobs:
//!  1. A small, fully functional in-host UI so the plugin is playable/automatable
//!     even if the Compose Desktop UI is never launched.
//!  2. The *bridge* that lets the external Compose UI participate in the host
//!     parameter system. nih-plug only allows parameter writes (so the host can
//!     record automation and persist them) through a `ParamSetter`, which is only
//!     available from an editor on the GUI thread. Compose edits arrive over IPC,
//!     get queued as [`ParamEdit`]s, and are applied here.

use crate::{CodecParams, Division, ParamEdit, ParamId, Shared, NUM_PARAMS};
use crossbeam_channel::Receiver;
use nih_plug::prelude::{Editor, Param, ParamSetter};
use nih_plug_egui::{create_egui_editor, egui, widgets};
use std::sync::atomic::Ordering;
use std::sync::Arc;

struct EditorState {
    params: Arc<CodecParams>,
    shared: Arc<Shared>,
    edit_rx: Receiver<ParamEdit>,
    /// Open-gesture flags per param (indexed by `ParamId as usize`), so begin/set/end
    /// stays well-formed even if an IPC message is dropped.
    in_gesture: [bool; NUM_PARAMS],
}

pub(crate) fn create(
    params: Arc<CodecParams>,
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
            in_gesture: [false; NUM_PARAMS],
        },
        |_, _| {},
        move |egui_ctx, setter, state| {
            drain_edits(state, setter);

            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.heading("Codec — Granular");

                let bpm = f32::from_bits(state.shared.bpm.load(Ordering::Relaxed));
                let grains = state.shared.active_grains.load(Ordering::Relaxed);
                ui.horizontal(|ui| {
                    ui.label(if bpm > 0.0 {
                        format!("{bpm:.1} BPM")
                    } else {
                        "no tempo".to_string()
                    });
                    ui.separator();
                    ui.label(format!("{grains} grains"));
                    ui.separator();
                    ui.label(if state.shared.ui_connected.load(Ordering::Relaxed) {
                        "Compose: connected"
                    } else {
                        "Compose: —"
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let p = &state.params;
                    slider(ui, "Density", &p.density, setter);
                    slider(ui, "Grain Size", &p.size, setter);
                    slider(ui, "Position", &p.position, setter);
                    slider(ui, "Spray", &p.spray, setter);
                    slider(ui, "Pitch", &p.pitch, setter);
                    slider(ui, "Pitch Spread", &p.pitch_spread, setter);
                    slider(ui, "Pan Spread", &p.pan_spread, setter);
                    slider(ui, "Feedback", &p.feedback, setter);
                    slider(ui, "Mix", &p.mix, setter);
                    ui.separator();
                    slider(ui, "Sync", &p.sync, setter);
                    slider(ui, "Division", &p.division, setter);
                    slider(ui, "Reverse", &p.reverse, setter);
                });

                ui.separator();
                ui.small("Optional richer UI: cd ui && ./gradlew run");
                ui.small("Keep this window open to record Compose edits as automation.");
            });

            egui_ctx.request_repaint();
        },
    )
}

fn slider<P: Param>(ui: &mut egui::Ui, label: &str, param: &P, setter: &ParamSetter) {
    ui.label(label);
    ui.add(widgets::ParamSlider::for_param(param, setter));
}

fn drain_edits(state: &mut EditorState, setter: &ParamSetter) {
    while let Ok(edit) = state.edit_rx.try_recv() {
        match edit {
            ParamEdit::Begin(id) => begin(state, setter, id),
            ParamEdit::Set(id, value) => {
                begin(state, setter, id); // defensive: ensure a gesture is open
                set(state, setter, id, value);
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
    let p = &state.params;
    match id {
        ParamId::Density => setter.begin_set_parameter(&p.density),
        ParamId::Size => setter.begin_set_parameter(&p.size),
        ParamId::Position => setter.begin_set_parameter(&p.position),
        ParamId::Spray => setter.begin_set_parameter(&p.spray),
        ParamId::Pitch => setter.begin_set_parameter(&p.pitch),
        ParamId::PitchSpread => setter.begin_set_parameter(&p.pitch_spread),
        ParamId::PanSpread => setter.begin_set_parameter(&p.pan_spread),
        ParamId::Feedback => setter.begin_set_parameter(&p.feedback),
        ParamId::Mix => setter.begin_set_parameter(&p.mix),
        ParamId::Sync => setter.begin_set_parameter(&p.sync),
        ParamId::Reverse => setter.begin_set_parameter(&p.reverse),
        ParamId::Division => setter.begin_set_parameter(&p.division),
    }
}

fn set(state: &mut EditorState, setter: &ParamSetter, id: ParamId, v: f32) {
    let p = &state.params;
    match id {
        ParamId::Density => setter.set_parameter(&p.density, v),
        ParamId::Size => setter.set_parameter(&p.size, v),
        ParamId::Position => setter.set_parameter(&p.position, v),
        ParamId::Spray => setter.set_parameter(&p.spray, v),
        ParamId::Pitch => setter.set_parameter(&p.pitch, v),
        ParamId::PitchSpread => setter.set_parameter(&p.pitch_spread, v),
        ParamId::PanSpread => setter.set_parameter(&p.pan_spread, v),
        ParamId::Feedback => setter.set_parameter(&p.feedback, v),
        ParamId::Mix => setter.set_parameter(&p.mix, v),
        ParamId::Sync => setter.set_parameter(&p.sync, v >= 0.5),
        ParamId::Reverse => setter.set_parameter(&p.reverse, v >= 0.5),
        ParamId::Division => setter.set_parameter(&p.division, Division::from_idx(v as i32)),
    }
}

fn end(state: &mut EditorState, setter: &ParamSetter, id: ParamId) {
    if !state.in_gesture[id as usize] {
        return;
    }
    state.in_gesture[id as usize] = false;
    let p = &state.params;
    match id {
        ParamId::Density => setter.end_set_parameter(&p.density),
        ParamId::Size => setter.end_set_parameter(&p.size),
        ParamId::Position => setter.end_set_parameter(&p.position),
        ParamId::Spray => setter.end_set_parameter(&p.spray),
        ParamId::Pitch => setter.end_set_parameter(&p.pitch),
        ParamId::PitchSpread => setter.end_set_parameter(&p.pitch_spread),
        ParamId::PanSpread => setter.end_set_parameter(&p.pan_spread),
        ParamId::Feedback => setter.end_set_parameter(&p.feedback),
        ParamId::Mix => setter.end_set_parameter(&p.mix),
        ParamId::Sync => setter.end_set_parameter(&p.sync),
        ParamId::Reverse => setter.end_set_parameter(&p.reverse),
        ParamId::Division => setter.end_set_parameter(&p.division),
    }
}
