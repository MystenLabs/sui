// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "gas-profiler")]
use move_vm_config::runtime::VMProfilerConfig;
#[cfg(feature = "gas-profiler")]
use once_cell::sync::Lazy;
#[cfg(feature = "gas-profiler")]
use serde::Serialize;
#[cfg(feature = "gas-profiler")]
use std::collections::BTreeMap;

#[cfg(feature = "gas-profiler")]
const MOVE_VM_PROFILER_ENV_VAR_NAME: &str = "MOVE_VM_PROFILE";

#[cfg(feature = "gas-profiler")]
static PROFILER_ENABLED: Lazy<bool> =
    Lazy::new(|| std::env::var(MOVE_VM_PROFILER_ENV_VAR_NAME).is_ok());

#[cfg(feature = "gas-profiler")]
#[derive(Debug, Clone, Serialize)]
pub struct FrameName {
    name: String,
    file: String,
}

#[cfg(feature = "gas-profiler")]
#[derive(Debug, Clone, Serialize)]
pub struct Shared {
    frames: Vec<FrameName>,

    #[serde(skip)]
    frame_table: BTreeMap<String, usize>,
}

#[cfg(feature = "gas-profiler")]
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    #[serde(rename(serialize = "type"))]
    ty: String,
    frame: u64,
    at: u64,
}

#[cfg(feature = "gas-profiler")]
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

#[cfg(feature = "gas-profiler")]
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
    pub config: VMProfilerConfig,
    #[serde(skip)]
    finished: bool,
}

#[cfg(feature = "gas-profiler")]
impl GasProfiler {
    // Used by profiler viz tool
    const OPEN_FRAME_IDENT: &str = "O";
    const CLOSE_FRAME_IDENT: &str = "C";

    const TOP_LEVEL_FRAME_NAME: &str = "root";

    pub fn init(config: &VMProfilerConfig, name: String, start_gas: u64) -> Self {
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
            start_gas,
            config: config.clone(),
            finished: false,
        };
        profile_open_frame_impl!(
            Some(&mut prof),
            Self::TOP_LEVEL_FRAME_NAME.to_string(),
            start_gas
        );
        prof
    }

    pub fn init_default_cfg(name: String, start_gas: u64) -> Self {
        Self::init(&VMProfilerConfig::default(), name, start_gas)
    }

    fn profile_name(&self) -> String {
        self.name.clone()
    }

    pub fn short_name(s: &String) -> String {
        s.split("::").last().unwrap_or(s).to_string()
    }

    fn is_metered(&self) -> bool {
        (self.profiles[0].end_value != 0) && (self.start_gas != 0)
    }

    fn start_gas(&self) -> u64 {
        self.start_gas
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

    pub fn open_frame(&mut self, frame_name: String, metadata: String, gas_start: u64) {
        if !(self.config.enabled || *PROFILER_ENABLED) || self.start_gas == 0 {
            return;
        }

        let frame_idx = self.add_frame(metadata.clone(), frame_name, metadata);
        let start = self.start_gas();

        self.profiles[0].events.push(Event {
            ty: Self::OPEN_FRAME_IDENT.to_string(),
            frame: frame_idx,
            at: start - gas_start,
        });
    }

    pub fn close_frame(&mut self, frame_name: String, metadata: String, gas_end: u64) {
        if !(self.config.enabled || *PROFILER_ENABLED) || self.start_gas == 0 {
            return;
        }
        let frame_idx = self.add_frame(metadata.clone(), frame_name, metadata);
        let start = self.start_gas();

        self.profiles[0].events.push(Event {
            ty: Self::CLOSE_FRAME_IDENT.to_string(),
            frame: frame_idx,
            at: start - gas_end,
        });
        self.profiles[0].end_value = start - gas_end;
    }

    pub fn to_file(&self) {
        if !(self.config.enabled || *PROFILER_ENABLED) || !self.is_metered() {
            return;
        }

        let mut p = self.config.base_path.clone();
        if let Some(f) = &self.config.full_path {
            p = f.clone();
        } else {
            // Get the unix timestamp
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Clock may have gone backwards")
                .as_nanos();
            p.push(format!("gas_profile_{}_{}.json", self.profile_name(), now));
        }
        let path_str = p.as_os_str().to_string_lossy().to_string();

        let mut file = std::fs::File::create(p).expect("Unable to create file");

        let json = serde_json::to_string_pretty(&self).expect("Unable to serialize profile");
        std::io::Write::write_all(&mut file, json.as_bytes()).expect("Unable to write to file");
        println!("Gas profile written to file: {}", path_str);
    }

    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let end_gas = self.start_gas() - self.profiles[0].end_value;
        let mut q = Some(self);
        profile_close_frame_impl!(&mut q, Self::TOP_LEVEL_FRAME_NAME.to_string(), end_gas);
        profile_dump_file!(q.unwrap());
    }
}

#[cfg(feature = "gas-profiler")]
impl Drop for GasProfiler {
    fn drop(&mut self) {
        self.finish();
    }
}

#[macro_export]
macro_rules! profile_open_frame {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "gas-profiler")]
        {
            let gas_rem = $gas_meter.remaining_gas().into();
            move_vm_profiler::profile_open_frame_impl!(
                $gas_meter.get_profiler_mut(),
                $frame_name,
                gas_rem
            )
        }
    };
}

#[macro_export]
macro_rules! profile_open_frame_impl {
    ($profiler:expr, $frame_name:expr, $gas_rem:expr) => {
        #[cfg(feature = "gas-profiler")]
        {
            if let Some(profiler) = $profiler {
                let name = if !profiler.config.use_long_function_name {
                    GasProfiler::short_name(&$frame_name)
                } else {
                    $frame_name
                };
                profiler.open_frame(name, $frame_name, $gas_rem)
            }
        }
    };
}

#[macro_export]
macro_rules! profile_close_frame {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "gas-profiler")]
        {
            let gas_rem = $gas_meter.remaining_gas().into();
            move_vm_profiler::profile_close_frame_impl!(
                $gas_meter.get_profiler_mut(),
                $frame_name,
                gas_rem
            )
        }
    };
}

#[macro_export]
macro_rules! profile_close_frame_impl {
    ($profiler:expr, $frame_name:expr, $gas_rem:expr) => {
        #[cfg(feature = "gas-profiler")]
        {
            if let Some(profiler) = $profiler {
                let name = if !profiler.config.use_long_function_name {
                    GasProfiler::short_name(&$frame_name)
                } else {
                    $frame_name.clone()
                };
                profiler.close_frame(name, $frame_name, $gas_rem)
            }
        }
    };
}

#[macro_export]
macro_rules! profile_open_instr {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "gas-profiler")]
        {
            let gas_rem = $gas_meter.remaining_gas().into();
            if let Some(profiler) = $gas_meter.get_profiler_mut() {
                if profiler.config.track_bytecode_instructions {
                    profiler.open_frame($frame_name.clone(), $frame_name, gas_rem)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! profile_close_instr {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "gas-profiler")]
        {
            let gas_rem = $gas_meter.remaining_gas().into();
            if let Some(profiler) = $gas_meter.get_profiler_mut() {
                if profiler.config.track_bytecode_instructions {
                    profiler.close_frame($frame_name.clone(), $frame_name, gas_rem)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! profile_dump_file {
    ($profiler:expr) => {
        #[cfg(feature = "gas-profiler")]
        $profiler.to_file()
    };
}
