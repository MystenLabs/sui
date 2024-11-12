// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use move_vm_config::runtime::VMProfilerConfig;
use serde::Serialize;
use std::collections::BTreeMap;

#[cfg(feature = "tracing")]
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
    pub config: Option<VMProfilerConfig>,
    #[serde(skip)]
    finished: bool,
}

#[cfg(feature = "tracing")]
impl GasProfiler {
    // Used by profiler viz tool
    const OPEN_FRAME_IDENT: &'static str = "O";
    const CLOSE_FRAME_IDENT: &'static str = "C";

    const TOP_LEVEL_FRAME_NAME: &'static str = "root";

    #[cfg(feature = "tracing")]
    pub fn init(config: &Option<VMProfilerConfig>, name: String, start_gas: u64) -> Self {
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

    #[cfg(feature = "tracing")]
    pub fn init_default_cfg(name: String, start_gas: u64) -> Self {
        Self::init(
            &VMProfilerConfig::get_default_config_if_enabled(),
            name,
            start_gas,
        )
    }

    #[cfg(feature = "tracing")]
    pub fn short_name(s: &String) -> String {
        s.split("::").last().unwrap_or(s).to_string()
    }

    #[cfg(feature = "tracing")]
    fn is_metered(&self) -> bool {
        (self.profiles[0].end_value != 0) && (self.start_gas != 0)
    }

    #[cfg(feature = "tracing")]
    fn start_gas(&self) -> u64 {
        self.start_gas
    }

    #[cfg(feature = "tracing")]
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

    #[cfg(feature = "tracing")]
    pub fn open_frame(&mut self, frame_name: String, metadata: String, gas_start: u64) {
        if self.config.is_none() || self.start_gas == 0 {
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

    #[cfg(feature = "tracing")]
    pub fn close_frame(&mut self, frame_name: String, metadata: String, gas_end: u64) {
        if self.config.is_none() || self.start_gas == 0 {
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

    #[cfg(feature = "tracing")]
    pub fn to_file(&self) {
        use std::ffi::{OsStr, OsString};
        use std::fs::File;
        use std::io::Write;
        use std::time::SystemTime;

        let Some(config) = &self.config else {
            return;
        };
        if !self.is_metered() {
            info!("No meaningful gas usage for this transaction, it may be a system transaction");
            return;
        }

        let mut p = config.full_path.clone();
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
    }

    #[cfg(feature = "tracing")]
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

#[cfg(feature = "tracing")]
impl Drop for GasProfiler {
    fn drop(&mut self) {
        self.finish();
    }
}

#[macro_export]
macro_rules! profile_open_frame {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "tracing")]
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
        #[cfg(feature = "tracing")]
        {
            if let Some(profiler) = $profiler {
                if let Some(config) = &profiler.config {
                    let name = if !config.use_long_function_name {
                        $crate::GasProfiler::short_name(&$frame_name)
                    } else {
                        $frame_name
                    };
                    profiler.open_frame(name, $frame_name, $gas_rem)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! profile_close_frame {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "tracing")]
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
        #[cfg(feature = "tracing")]
        {
            if let Some(profiler) = $profiler {
                if let Some(config) = &profiler.config {
                    let name = if !config.use_long_function_name {
                        $crate::GasProfiler::short_name(&$frame_name)
                    } else {
                        $frame_name
                    };
                    profiler.close_frame(name, $frame_name, $gas_rem)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! profile_open_instr {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "tracing")]
        {
            let gas_rem = $gas_meter.remaining_gas().into();
            if let Some(profiler) = $gas_meter.get_profiler_mut() {
                if let Some(config) = &profiler.config {
                    if config.track_bytecode_instructions {
                        profiler.open_frame($frame_name.clone(), $frame_name, gas_rem)
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! profile_close_instr {
    ($gas_meter:expr, $frame_name:expr) => {
        #[cfg(feature = "tracing")]
        {
            let gas_rem = $gas_meter.remaining_gas().into();
            if let Some(profiler) = $gas_meter.get_profiler_mut() {
                if let Some(config) = &profiler.config {
                    if config.track_bytecode_instructions {
                        profiler.close_frame($frame_name.clone(), $frame_name, gas_rem)
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! profile_dump_file {
    ($profiler:expr) => {
        #[cfg(feature = "tracing")]
        $profiler.to_file()
    };
}

#[cfg(feature = "tracing")]
#[macro_export]
macro_rules! tracing_feature_enabled {
    ($($tt:tt)*) => {
        if cfg!(feature = "tracing") {
            $($tt)*
        }
    };
}

#[cfg(not(feature = "tracing"))]
#[macro_export]
macro_rules! tracing_feature_enabled {
    ( $( $tt:tt )* ) => {};
}

#[cfg(not(feature = "tracing"))]
#[macro_export]
macro_rules! tracing_feature_disabled {
    ($($tt:tt)*) => {
        if !cfg!(feature = "tracing") {
            $($tt)*
        }
    };
}

#[cfg(feature = "tracing")]
#[macro_export]
macro_rules! tracing_feature_disabled {
    ( $( $tt:tt )* ) => {};
}
