// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use fastx_types::{error, messages, serialize};
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};
use std::{fs::File, io::Write};
use structopt::{clap::arg_enum, StructOpt};

fn get_registry() -> Result<Registry> {
    let mut tracer = Tracer::new(TracerConfig::default());
    let samples = Samples::new();
    // 1. Record samples for types with custom deserializers.
    // tracer.trace_value(&mut samples, ...)?;

    // 2. Trace the main entry point(s) + every enum separately.
    tracer.trace_type::<messages::Address>(&samples)?;
    tracer.trace_type::<error::SuiError>(&samples)?;
    tracer.trace_type::<serialize::SerializedMessage>(&samples)?;
    tracer.registry()
}

arg_enum! {
#[derive(Debug, StructOpt, Clone, Copy)]
enum Action {
    Print,
    Test,
    Record,
}
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Sui format generator",
    about = "Trace serde (de)serialization to generate format descriptions for Sui types"
)]
struct Options {
    #[structopt(possible_values = &Action::variants(), default_value = "Print", case_insensitive = true)]
    action: Action,
}

const FILE_PATH: &str = "sui_core/tests/staged/sui.yaml";

fn main() {
    let options = Options::from_args();
    let registry = get_registry().unwrap();
    match options.action {
        Action::Print => {
            let content = serde_yaml::to_string(&registry).unwrap();
            println!("{}", content);
        }
        Action::Record => {
            let content = serde_yaml::to_string(&registry).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{}", content).unwrap();
        }
        Action::Test => {
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let content = serde_yaml::to_string(&registry).unwrap() + "\n";
            assert_str::assert_str_eq!(&reference, &content);
        }
    }
}
