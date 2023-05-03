use arbitrary::{Arbitrary, Unstructured};

use afl::fuzz;
use clap::Parser;
use move_bytecode_verifier::meter::DummyMeter;
use std::collections::BTreeMap;
use std::process;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_verifier::{
    entry_points_verifier, global_storage_access_verifier, id_leak_verifier,
    one_time_witness_verifier, private_generics, struct_with_key_verifier,
    verifier as sui_bytecode_verifier,
};

#[derive(Parser)]
struct Args {
    #[clap(short, long, default_value = "raw-bytes")]
    // raw-bytes, arbitrary-bytes, source
    input_format: String,

    #[clap(short, long, default_value = "sui-verifier")]
    target: String,

    #[clap(short, long, action)]
    debug: bool,
}

fn main() {
    let args = Args::parse();

    // Use the fuzz! macro because it promotes panics to `abort` signals which
    // AFL needs to detect a crash. Alternatively set `abort = "panic"` for
    // profiles in Cargo.toml.
    fuzz!(|input: &[u8]| {
        match args.target.as_str() {
            "sui-verifier" | _ => {
                let m = match args.input_format.as_str() {
                    "arbitrary" => {
                        let mut unstructured = Unstructured::new(input);
                        let Ok(m) = move_binary_format::file_format::CompiledModule::arbitrary(&mut unstructured) else { return };
                        m
                    }
                    "raw-bytes" | _ => {
                        let Ok(m) = move_binary_format::file_format::CompiledModule::deserialize(&input) else { return };
                        m
                    }
                };
                if args.debug {
                    // Print human-readable representation of input.
                    dbg!(m.to_owned());
                };
                let move_result = move_bytecode_verifier::verify_module(&m);
                if let Ok(()) = move_result {
                    let sui_result = sui_bytecode_verifier::verify_module(
                        &ProtocolConfig::get_for_version(ProtocolVersion::MAX),
                        &m,
                        &BTreeMap::new(),
                    );
                    if let Ok(()) = sui_result {
                        process::exit(0);
                    } else {
                        dbg!("sui verifier failure");
                        dbg!(sui_result.err().unwrap());
                    }
                } else {
                    if args.debug {
                        dbg!("move verifier failure");
                        dbg!(move_result.err().unwrap());
                    }
                }
                process::exit(1); // Invalid, verification fails.
            }
        }
    })
}
