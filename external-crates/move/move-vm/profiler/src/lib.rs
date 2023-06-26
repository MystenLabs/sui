// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_vm_config::runtime::VMProfilerConfig;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[cfg(any(debug_assertions, feature = "debugging"))]
const MOVE_VM_PROFILER_ENV_VAR_NAME: &str = "MOVE_VM_PROFILE";

#[cfg(any(debug_assertions, feature = "debugging"))]
static PROFILER_ENABLED: Lazy<bool> =
    Lazy::new(|| std::env::var(MOVE_VM_PROFILER_ENV_VAR_NAME).is_ok());

#[derive(Debug, Clone, Serialize)]
pub struct FrameName {
    name: String,
}
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GasProfile {
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
    pub output_base_path: PathBuf,
}

impl GasProfile {
    pub fn init(config: &VMProfilerConfig, name: String, start_gas: u64) -> Self {
        GasProfile {
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
            start_gas,
            output_base_path: config.base_path.clone(),
        }
    }

    fn get_profile_name(&self) -> String {
        self.name.clone()
    }

    fn is_metered(&self) -> bool {
        self.profiles[0].end_value != 0
    }

    fn get_start_gas(&self) -> u64 {
        self.start_gas
    }

    fn add_frame(&mut self, frame_name: String) -> u64 {
        *self
            .shared
            .frame_table
            .entry(frame_name.clone())
            .or_insert({
                let val = self.shared.frames.len() as u64;
                self.shared.frames.push(FrameName { name: frame_name });
                val as usize
            }) as u64
    }

    pub fn open_frame(&mut self, frame_name: String, gas_start: u64) {
        if !*PROFILER_ENABLED {
            return;
        }

        let frame_idx = self.add_frame(frame_name);
        let start = self.get_start_gas();
        self.profiles[0].events.push(Event {
            ty: "O".to_string(),
            frame: frame_idx,
            at: start - gas_start,
        });
    }

    pub fn close_frame(&mut self, frame_name: String, gas_end: u64) {
        if !*PROFILER_ENABLED {
            return;
        }
        let frame_idx = self.add_frame(frame_name);
        let start = self.get_start_gas();

        self.profiles[0].events.push(Event {
            ty: "C".to_string(),
            frame: frame_idx,
            at: start - gas_end,
        });
        self.profiles[0].end_value = start - gas_end;
    }

    pub fn to_file(&self) {
        if !*PROFILER_ENABLED || !self.is_metered() {
            return;
        }
        // Get the unix timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Clock may have gone backwards")
            .as_nanos();

        let mut p = self.output_base_path.clone();
        p.push(format!(
            "gas_profile_{}_{}.json",
            self.get_profile_name(),
            now
        ));
        let path_str = p.as_os_str().to_string_lossy().to_string();
        let mut file = std::fs::File::create(p).expect("Unable to create file");

        let json = serde_json::to_string_pretty(&self).expect("Unable to serialize profile");
        std::io::Write::write_all(&mut file, json.as_bytes()).expect("Unable to write to file");
        println!("Gas profile written to file: {}", path_str);
    }
}

#[macro_export]
macro_rules! profile_open_frame {
    ($profiler:expr, $frame_name:expr, $gas_rem:expr) => {
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $profiler.open_frame($frame_name, $gas_rem)
    };
}

#[macro_export]
macro_rules! profile_close_frame {
    ($profiler:expr, $frame_name:expr, $gas_rem:expr) => {
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $profiler.close_frame($frame_name, $gas_rem)
    };
}

#[macro_export]
macro_rules! profile_dump_file {
    ($profiler:expr) => {
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $profiler.to_file()
    };
}
