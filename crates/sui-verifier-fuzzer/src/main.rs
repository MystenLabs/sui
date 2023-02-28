use arbitrary::{Arbitrary, Unstructured};

use afl::fuzz;
use std::process;
use std::{collections::BTreeMap, env};
use sui_verifier::{
    entry_points_verifier, global_storage_access_verifier, id_leak_verifier,
    one_time_witness_verifier, private_generics, struct_with_key_verifier,
    verifier as sui_bytecode_verifier,
};

fn main() {
    let mut args: Vec<String> = env::args().collect();
    match &args[..] {
        [] => unreachable!(""),
        [_] => {
            args.push("".to_owned());
            args.push("".to_owned())
        }
        [_, option] => {
            args.push(option.to_owned());
            args.push("".to_owned())
        }
        _ => (),
    };
    let debug = match args[2].as_str() {
        "--debug" | "-d" => true,
        _ => false,
    };

    // Use the fuzz! macro because it promotes panics to `abort` signals which
    // AFL needs to detect a crash. Alternatively is set `abort = "panic"` for
    // profiles in Cargo.toml.
    fuzz!(|input: &[u8]| {
        let mut unstructured = Unstructured::new(input);
        if let Ok(m) = move_binary_format::file_format::CompiledModule::arbitrary(&mut unstructured)
        {
            if debug {
                // Print human-readable representation of input.
                dbg!(m.to_owned());
            };

            match args[1].as_str() {
                // Only fuzz core move.
                // For individual passes see https://github.com/move-language/move/blob/3528240a96ee06bf2e1528066456ff324e43b7f5/language/move-bytecode-verifier/src/verifier.rs#L53-L72.
                "--core-move" => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        process::exit(0);
                    }
                }

                // Fuzz individual sui move passes.
                // Based on https://github.com/MystenLabs/sui/blob/0b66c5c3c7cb159bb9372018302cbc451dfd980c/crates/sui-verifier/src/verifier.rs#L15-L25.
                "--sui-move-struct-with-key" => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) = struct_with_key_verifier::verify_module(&m) {
                            process::exit(0);
                        }
                    }
                }
                "--sui-move-global-storage-access" => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) = global_storage_access_verifier::verify_module(&m) {
                            process::exit(0);
                        }
                    }
                }
                "--sui-move-id-leak" => {
                    // Note: this pass contains an invariant that expects the
                    // `global-storage-access` pass to run before it, so it is not a
                    // good candidate to run individually.
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) = id_leak_verifier::verify_module(&m) {
                            process::exit(0);
                        }
                    }
                }
                "--sui-move-entry-points" => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) = entry_points_verifier::verify_module(&m, &BTreeMap::new()) {
                            process::exit(0);
                        }
                    }
                }
                "--sui-move-private-generics" => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) = private_generics::verify_module(&m) {
                            process::exit(0);
                        }
                    }
                }
                "--sui-move-one-time-witness" => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) =
                            one_time_witness_verifier::verify_module(&m, &BTreeMap::new())
                        {
                            process::exit(0);
                        }
                    }
                }
                // Fuzz all passes in both core move and sui verifier.
                // Based on https://github.com/MystenLabs/sui/blob/main/crates/sui-framework-build/src/compiled_package.rs#L155-L166.
                "sui-move" | _ => {
                    if let Ok(()) = move_bytecode_verifier::verify_module(&m) {
                        if let Ok(()) = sui_bytecode_verifier::verify_module(&m, &BTreeMap::new()) {
                            process::exit(0);
                        }
                    }
                }
            }
        }
        process::exit(1); // Invalid, verification fails.
    })
}
