// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_trace_format::format::{Frame, MoveTraceReader, TraceEvent};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct FrameName {
    name: String,
    file: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct Shared {
    frames: Vec<FrameName>,

    #[serde(skip)]
    frame_table: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(rename(serialize = "type"))]
    ty: String,
    frame: u64,
    at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(rename(serialize = "type"))]
    ty: String,
    name: String,
    unit: String,
    start_value: u64,
    end_value: u64,
    events: Vec<Event>,
}

#[derive(Clone, Debug, Default)]
pub struct ProfilerConfig {
    /// User configured full output directory for the gas profile
    pub output_dir: Option<PathBuf>,
    /// Whether or not to use the long name for functions
    pub use_long_function_name: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GasProfiler {
    exporter: String,
    name: String,
    active_profile_index: u64,
    #[serde(rename(serialize = "$schema"))]
    schema: String,
    shared: Shared,
    profiles: Vec<Profile>,

    #[serde(skip)]
    pub current_gas: Option<u64>,
    #[serde(skip)]
    pub config: ProfilerConfig,
    #[serde(skip)]
    pub frames: Vec<(usize, Frame)>,
}

impl GasProfiler {
    // Used by profiler viz tool
    const OPEN_FRAME_IDENT: &'static str = "O";
    const CLOSE_FRAME_IDENT: &'static str = "C";

    pub fn init(config: ProfilerConfig, name: String) -> Self {
        GasProfiler {
            exporter: "speedscope@1.15.2".to_string(),
            name: name.clone(),
            active_profile_index: 0,
            schema: "https://www.speedscope.app/file-format-schema.json".to_string(),
            shared: Shared {
                frames: vec![],
                frame_table: BTreeMap::new(),
            },
            profiles: vec![Profile {
                ty: "evented".to_string(),
                name,
                unit: "none".to_string(),
                start_value: 0,
                end_value: 0,
                events: vec![],
            }],
            current_gas: None,
            config: config.clone(),
            frames: Vec::new(),
        }
    }

    pub fn init_default_cfg(name: String) -> Self {
        Self::init(ProfilerConfig::default(), name)
    }

    pub fn short_name(s: &str) -> String {
        s.split("::").last().unwrap_or(s).to_string()
    }

    fn frame_name(&self, name: &str) -> String {
        if self.config.use_long_function_name {
            name.to_string()
        } else {
            Self::short_name(name)
        }
    }

    fn get_gas_span(&mut self, current_gas: u64) -> u64 {
        if let Some(curr_gas_state) = &mut self.current_gas {
            curr_gas_state.saturating_sub(current_gas)
        } else {
            self.current_gas = Some(current_gas);
            0
        }
    }

    fn add_frame(
        &mut self,
        frame_name: String,
        frame_display_name: String,
        metadata: String,
    ) -> u64 {
        match self.shared.frame_table.get(frame_name.as_str()) {
            Some(idx) => *idx as u64,
            None => {
                let val = self.shared.frames.len() as u64;
                self.shared.frames.push(FrameName {
                    name: frame_display_name,
                    file: metadata,
                });
                self.shared.frame_table.insert(frame_name, val as usize);
                val
            }
        }
    }

    pub fn open_frame(&mut self, frame_name: String, metadata: String, gas_remaining: u64) {
        let name = self.frame_name(&frame_name);
        let frame_idx = self.add_frame(frame_name, name, metadata);
        let at = self.get_gas_span(gas_remaining);

        self.profiles[0].events.push(Event {
            ty: Self::OPEN_FRAME_IDENT.to_string(),
            frame: frame_idx,
            at,
        });
    }

    pub fn close_frame(&mut self, frame_name: String, metadata: String, gas_remaining: u64) {
        let name = self.frame_name(&frame_name);
        let frame_idx = self.add_frame(frame_name, name, metadata);
        let at = self.get_gas_span(gas_remaining);

        self.profiles[0].events.push(Event {
            ty: Self::CLOSE_FRAME_IDENT.to_string(),
            frame: frame_idx,
            at,
        });
        self.profiles[0].end_value = at;
    }

    pub fn generate_from_trace<R: std::io::Read>(&mut self, trace: MoveTraceReader<R>) {
        let mut last_gas_left = 0u64;
        for event in trace {
            let event = event.expect("Failed to read trace event");
            match event {
                TraceEvent::Instruction { gas_left, .. } => {
                    last_gas_left = gas_left;
                }
                TraceEvent::Effect(..) | TraceEvent::External(..) => (),
                TraceEvent::OpenFrame { frame, gas_left } => {
                    self.open_frame(Self::trace_name(&frame), "".to_string(), gas_left);
                    self.frames.push((frame.frame_id, *frame));
                    last_gas_left = gas_left;
                }
                TraceEvent::CloseFrame {
                    frame_id,
                    return_: _,
                    gas_left,
                } => {
                    let (open_frame_id, frame) = self.frames.pop().expect("Frame stack underflow");
                    assert_eq!(
                        frame_id, open_frame_id,
                        "Mismatched frame IDs, this shouldn't be possible"
                    );
                    self.close_frame(Self::trace_name(&frame), "".to_string(), gas_left);
                    last_gas_left = gas_left;
                }
            }
        }

        // If we have any dangling frames in the trace (because execution aborted for some reason),
        // close them now so the profile is well-formed. All the closing frames will have the same
        // gas left as the last event.
        self.close_dangling_frames(last_gas_left);
    }

    fn close_dangling_frames(&mut self, last_gas_left: u64) {
        while let Some((_, frame)) = self.frames.pop() {
            self.close_frame(Self::trace_name(&frame), "".to_string(), last_gas_left);
        }
    }

    fn trace_name(frame: &Frame) -> String {
        format!(
            "{}::{}::{}",
            frame.version_id.to_canonical_display(true),
            frame.module.name(),
            frame.function_name
        )
    }

    fn filename_trim_all_extensions(path: &Path) -> Option<String> {
        path.file_name().and_then(|name| name.to_str()).map(|s| {
            s.split_once('.')
                .map(|(base, _)| base)
                .unwrap_or(s)
                .to_string()
        })
    }

    pub fn save_profile(&self) {
        let name = Self::filename_trim_all_extensions(Path::new(&self.name))
            .unwrap_or_else(|| self.name.clone());
        let mut p = self
            .config
            .output_dir
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        p.push(format!("gas_profile_{}.json", name));
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("Unable to create parent directory");
        }

        println!("Saving gas profile to: {}", p.display());
        let mut file = File::create(&p).expect("Unable to create file");

        let json = serde_json::to_string_pretty(&self).expect("Unable to serialize profile");
        file.write_all(json.as_bytes())
            .expect("Unable to write to file");
        tracing::info!("Gas profile written to file: {}", p.display());
    }
}
