// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_trace_format::format::{MoveTraceReader, TraceEvent};
use serde::Serialize;
use std::{collections::BTreeMap, io::Read, iter::Peekable, path::PathBuf};
use tracing::info;

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

#[allow(dead_code)]
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
    pub start_gas: u64,
    #[serde(skip)]
    pub config: GasProfilerConfig,
    #[serde(skip)]
    finished: bool,
}

#[derive(Debug, Clone)]
pub struct GasProfilerConfig {
    pub output_path: PathBuf,
    pub use_long_function_name: bool,
}

impl GasProfiler {
    const OPEN_FRAME_IDENT: &'static str = "O";
    const CLOSE_FRAME_IDENT: &'static str = "C";

    pub fn write_profile_from_trace<R: Read>(
        config: GasProfilerConfig,
        name: String,
        trace: MoveTraceReader<R>,
    ) -> anyhow::Result<()> {
        let mut peek_iter = trace.peekable();
        let Some(Ok(TraceEvent::OpenFrame { gas_left, .. })) = peek_iter.peek() else {
            return Err(anyhow::anyhow!(
                "No open frame event found as first event in trace"
            ));
        };
        let mut prof = GasProfiler {
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
            start_gas: *gas_left,
            config,
            finished: false,
        };

        prof.build_profile(&mut peek_iter)?;
        prof.dump_file()
    }

    fn add_trace_frame(&mut self, fn_name: String) -> u64 {
        match self.shared.frame_table.get(fn_name.as_str()) {
            Some(idx) => *idx as u64,
            None => {
                let val = self.shared.frames.len() as u64;
                self.shared.frames.push(FrameName {
                    name: fn_name.clone(),
                    file: fn_name.clone(),
                });
                self.shared.frame_table.insert(fn_name, val as usize);
                val
            }
        }
    }

    fn build_profile<R: Read>(
        &mut self,
        trace: &mut Peekable<MoveTraceReader<R>>,
    ) -> anyhow::Result<()> {
        let mut frame_map = BTreeMap::new();
        for event in trace.into_iter() {
            match event? {
                TraceEvent::Effect(..)
                | TraceEvent::External(..)
                | TraceEvent::Instruction { .. } => (),
                TraceEvent::OpenFrame { frame, gas_left } => {
                    let fn_name = if self.config.use_long_function_name {
                        format!("{}::{}", frame.module, frame.function_name,)
                    } else {
                        format!("{}", frame.function_name)
                    };
                    let frame_idx = self.add_trace_frame(fn_name.clone());
                    let start_gas = self.start_gas();
                    frame_map.insert(frame.frame_id, frame_idx);
                    self.profiles[0].events.push(Event {
                        ty: Self::OPEN_FRAME_IDENT.to_string(),
                        frame: frame_idx,
                        at: start_gas - gas_left,
                    });
                }
                TraceEvent::CloseFrame {
                    frame_id,
                    return_: _,
                    gas_left,
                } => {
                    let profiler_frame_idx = &frame_map[&frame_id];
                    let start_gas = self.start_gas();
                    self.profiles[0].events.push(Event {
                        ty: Self::CLOSE_FRAME_IDENT.to_string(),
                        frame: *profiler_frame_idx,
                        at: start_gas - gas_left,
                    });
                    self.profiles[0].end_value = start_gas - gas_left;
                }
            }
        }
        self.finished = true;
        Ok(())
    }

    fn dump_file(&self) -> anyhow::Result<()> {
        use std::ffi::{OsStr, OsString};
        use std::fs::File;
        use std::io::Write;
        use std::time::SystemTime;

        if !self.is_metered() {
            info!("No meaningful gas usage for this transaction, it may be a system transaction");
            return Ok(());
        }

        let mut p = self.config.output_path.clone();
        let mut filename = OsString::new();
        filename.push(p.file_name().unwrap_or_else(|| OsStr::new("gas_profile")));
        filename.push("_");
        filename.push(self.name.clone());
        filename.push("_");
        filename.push(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("Error getting system time")
                .as_nanos()
                .to_string(),
        );
        filename.push(".");
        filename.push(p.extension().unwrap_or_else(|| OsStr::new("json")));
        p.set_file_name(filename);

        let mut file = File::create(&p).expect("Unable to create file");

        let json = serde_json::to_string_pretty(&self).expect("Unable to serialize profile");
        file.write_all(json.as_bytes())
            .expect("Unable to write to file");
        info!("Gas profile written to file: {}", p.display());
        Ok(())
    }

    fn is_metered(&self) -> bool {
        (self.profiles[0].end_value != 0) && (self.start_gas != 0)
    }

    fn start_gas(&self) -> u64 {
        self.start_gas
    }
}
