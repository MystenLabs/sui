use arbitrary::{Arbitrary, Unstructured};

use afl::fuzz;
use clap::Parser;
use move_bytecode_verifier::meter::DummyMeter;
use std::collections::BTreeMap;
use std::io::Read;
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
    /// raw-bytes, arbitrary-bytes, ir, source
    input_format: String,

    #[clap(short, long, default_value = "sui-verifier")]
    target: String,

    #[clap(long, action)]
    debug: bool,

    #[clap(long, action)]
    dry_run: bool,
}

fn main() {
    let args = Args::parse();
    let compiled_state = move_transactional_test_runner::framework::CompiledState::new(
        sui_transactional_test_runner::test_adapter::NAMED_ADDRESSES.clone(),
        Some(&*sui_transactional_test_runner::test_adapter::PRE_COMPILED),
        Some(move_compiler::shared::NumericalAddress::new(
            move_core_types::account_address::AccountAddress::ZERO.into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        )),
    );

    if args.dry_run {
        let mut handle = std::io::stdin().lock();
        let mut input = Vec::new();
        let _ = handle.read_to_end(&mut input);
        match args.target.as_str() {
            "move-binary-format" => {
                // raw-bytes implied
                let Ok(code) = std::str::from_utf8(&input) else { process::exit(1); };
                let m = move_ir_compiler::Compiler::new(compiled_state.dep_modules().collect())
                    .into_compiled_module(code)
                    .unwrap_or_else(|e| {
                        dbg!("no compiled module: {:#?}", e);
                        process::exit(1);
                    });
                dbg!("valid module {:#?}", m);
                process::exit(0)
            }
            "sui-verifier" => {
                let m = match args.input_format.as_str() {
                    /*
                    "arbitrary" => {
                        let mut unstructured = Unstructured::new(input);
                        let Ok(m) = move_binary_format::file_format::CompiledModule::arbitrary(&mut unstructured) else { process::exit(1); };
                        m
                    }
                    "ir" => {
                        let Ok(code) = std::str::from_utf8(input) else { process::exit(1); };
                        let Ok(m) = move_ir_compiler::Compiler::new(compiled_state.dep_modules().collect()).into_compiled_module(code) else { process::exit(1); };
                        m
                    }
                    */
                    "source" => {
                        let Ok(source) = std::str::from_utf8(&input) else { process::exit(1) };
                        let m = parse_source(source).unwrap_or_else(|e| { 
                            dbg!("no parse: {:#?}", e);
                            process::exit(1);
                        });
                        if m.0.is_empty() {
                            process::exit(1);
                        }
                        let move_compiler::compiled_unit::CompiledUnitEnum::Module(ref m) = m.0[0] else { process::exit(1); };
                        m.named_module.module.clone()
                    }
                    "raw-bytes" | _ => {
                        let m = move_binary_format::file_format::CompiledModule::deserialize_with_defaults(&input).unwrap_or_else(|e| {
                            dbg!("no deserialize module: {:#?}", e);
                            process::exit(1);
                       });
                        dbg!("valid module {:#?}", m.clone());
                        m
                    }
                };
                let move_result = move_bytecode_verifier::verify_module_unmetered(&m);
                if let Ok(()) = move_result {
                        let sui_result = sui_bytecode_verifier::sui_verify_module_unmetered(
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
                    };
                process::exit(0);
            }
            "move-compiler" | _ => {
                // source implied
                let Ok(source) = std::str::from_utf8(&input) else { process::exit(1) };
                let m = parse_source(source).unwrap_or_else(|e| {
                    dbg!("no compile: {:#?}", e);
                    process::exit(1)
                });
                dbg!("success compile {:#?}", m);
                process::exit(0)
            }
        }
    } else {
        // Use the fuzz! macro because it promotes panics to `abort` signals which
        // AFL needs to detect a crash. Alternatively set `abort = "panic"` for
        // profiles in Cargo.toml.
        fuzz!(|input: &[u8]| {
            match args.target.as_str() {
                "move-compiler" => {
                    // source implied
                    let Ok(source) = std::str::from_utf8(&input) else { process::exit(1) };
                    let Ok(_) = parse_source(source) else { process::exit(1) };
                    process::exit(0);
                }
                "move-binary-format" => {
                    // raw-bytes implied
                    let Ok(_) = move_binary_format::file_format::CompiledModule::deserialize_with_defaults(&input) else { process::exit(1); };
                    process::exit(0);
                }
                "sui-verifier" | _ => {
                    let m = match args.input_format.as_str() {
                        "arbitrary" => {
                            let mut unstructured = Unstructured::new(input);
                            let Ok(m) = move_binary_format::file_format::CompiledModule::arbitrary(&mut unstructured) else { process::exit(1); };
                            m
                        }
                        "ir" => {
                            let Ok(code) = std::str::from_utf8(input) else { process::exit(1); };
                            let Ok(m) = move_ir_compiler::Compiler::new(compiled_state.dep_modules().collect()).into_compiled_module(code) else { process::exit(1); };
                            m
                        }
                        "source" => {
                            let Ok(source) = std::str::from_utf8(&input) else { process::exit(1) };
                            let Ok(m) = parse_source(source) else { process::exit(1) };
                            if m.0.is_empty() {
                                process::exit(1);
                            }
                            let move_compiler::compiled_unit::CompiledUnitEnum::Module(ref m) = m.0[0] else { process::exit(1); };
                            m.named_module.module.clone() // XXX why do I need to clone here?
                        }
                        "raw-bytes" | _ => {
                            let Ok(m) = move_binary_format::file_format::CompiledModule::deserialize_with_defaults(&input) else { process::exit(1); };
                            m
                        }
                    };
                    if args.debug {
                        // Print human-readable representation of input.
                        dbg!(m.to_owned());
                    };
                    let move_result = move_bytecode_verifier::verify_module_unmetered(&m);
                    if let Ok(()) = move_result {
                        let sui_result = sui_bytecode_verifier::sui_verify_module_unmetered(
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
}

fn parse_source(
    source: &str,
) -> Result<
    (
        Vec<move_compiler::compiled_unit::AnnotatedCompiledUnit>,
        move_compiler::diagnostics::Diagnostics,
    ),
    move_compiler::diagnostics::Diagnostics,
> {
    let mut diags = move_compiler::diagnostics::Diagnostics::new();
    let hash = move_command_line_common::files::FileHash::new(&source);
    let source = match move_compiler::parser::comments::verify_string(hash, &source) {
        Err(ds) => {
            diags.extend(ds);
            return Err(diags);
        }
        Ok(()) => &source,
    };
    // See https://sourcegraph.com/github.com/MystenLabs/sui@6efdcb00fe67495d7e0318ee199c3298170ac27e/-/blob/external-crates/move/move-compiler/src/shared/mod.rs?L279
    let flags = move_compiler::shared::Flags::empty();
    let mut compilation_env = move_compiler::shared::CompilationEnv::new(flags);
    let (defs, comments) = match move_compiler::parser::syntax::parse_file_string(
        &mut compilation_env,
        hash,
        source,
    ) {
        Ok(defs_and_comments) => defs_and_comments,
        Err(ds) => {
            diags.extend(ds);
            return Err(diags);
        }
    };
    // let (defs, comments, diags, hash) = (defs, comments, diags, hash);

    // Start of parse_program https://sourcegraph.com/github.com/MystenLabs/sui/-/blob/external-crates/move/move-compiler/src/parser/mod.rs?L29
    let mut source_definitions = Vec::new();
    let mut source_comments = move_compiler::parser::comments::CommentMap::new();
    let mut named_address_maps = move_compiler::shared::NamedAddressMaps::new();
    named_address_maps.insert(
        default_fuzzing_addresses()
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect::<move_compiler::shared::NamedAddressMap>(),
    );
    source_definitions.extend(defs.into_iter().map(|def| {
        move_compiler::parser::ast::PackageDefinition {
            package: None, // TODO: add real name?
            named_address_map: move_compiler::shared::NamedAddressMapIndex(0),
            def,
        }
    }));
    source_comments.insert(hash, comments);
    // diags.extend(diags);
    let env_result = compilation_env.check_diags_at_or_above_severity(
        move_compiler::diagnostics::codes::Severity::BlockingError,
    );
    if let Err(env_diags) = env_result {
        diags.extend(env_diags)
    };

    // TODO: no need for loop
    for move_compiler::parser::ast::PackageDefinition {
        named_address_map: idx,
        def,
        ..
    } in source_definitions.iter_mut()
    {
        move_compiler::attr_derivation::derive_from_attributes(
            &mut compilation_env,
            named_address_maps.get(*idx),
            def,
        );
    }

    // TODO: maybe set dummy values for file and follow closely: https://sourcegraph.com/github.com/MystenLabs/sui/-/blob/external-crates/move/move-compiler/src/parser/mod.rs?L67

    let res = if diags.is_empty() {
        let pprog = move_compiler::parser::ast::Program {
            named_address_maps,
            source_definitions,
            lib_definitions: Vec::new(), // XXX: does this need something?
        };
        Ok((pprog, source_comments))
    } else {
        Err(diags)
    };

    // Start from run in compiler.rs: https://sourcegraph.com/github.com/MystenLabs/sui@dad2a431bfa2cf19d9e967d3067b5f7b54040005/-/blob/external-crates/move/move-compiler/src/command_line/compiler.rs?L196
    let res: Result<
        (
            Vec<move_compiler::compiled_unit::AnnotatedCompiledUnit>,
            move_compiler::diagnostics::Diagnostics,
        ),
        move_compiler::diagnostics::Diagnostics,
    > = res.and_then(|(pprog, comments)| {
        move_compiler::SteppedCompiler::new_at_parser(
            compilation_env,
            Some(&*sui_transactional_test_runner::test_adapter::PRE_COMPILED),
            pprog,
        )
        .run::<{ move_compiler::PASS_COMPILATION }>()
        .map(|compiler| (comments, compiler))
        .map(|(_comments, stepped)| stepped.into_compiled_units())
    });

    // Start from build in compiler.rs: https://sourcegraph.com/github.com/MystenLabs/sui@dad2a431bfa2cf19d9e967d3067b5f7b54040005/-/blob/external-crates/move/move-compiler/src/command_line/compiler.rs?L238

    res
}

fn default_fuzzing_addresses() -> BTreeMap<String, move_compiler::shared::NumericalAddress> {
    let mapping = [
        ("std", "0x1"),
        ("sui", "0x2"),
        ("sui_system", "0x3"),
        ("deepbook", "0xdee9"),
        ("M", "0x42"),
        ("A", "0x42"),
        ("B", "0x42"),
        ("K", "0x42"),
        ("test", "0x42"),
        ("Async", "0x42"),
        ("123", "0x42"),
        ("a", "0x42"),
        ("A", "0x42"),
        ("A0", "0x42"),
        ("A1", "0x42"),
        ("A2", "0x42"),
        ("A3", "0x42"),
        ("A4", "0x42"),
        ("AA", "0x42"),
        ("abc", "0x42"),
        ("adversarial", "0x42"),
        ("Async", "0x42"),
        ("b", "0x42"),
        ("B", "0x42"),
        ("bar", "0x42"),
        ("base", "0x42"),
        ("basics", "0x42"),
        ("baz", "0x42"),
        ("c", "0x42"),
        ("capy", "0x42"),
        ("CoinSwap", "0x42"),
        ("CoreFramework", "0x42"),
        ("deepbook", "0x42"),
        ("defi", "0x42"),
        ("depends", "0x42"),
        ("DiemFramework", "0x42"),
        ("e", "0x42"),
        ("Evm", "0x42"),
        ("examples", "0x42"),
        ("ExperimentalFramework", "0x42"),
        ("extensions", "0x42"),
        ("foo", "0x42"),
        ("games", "0x42"),
        ("GoldCoin", "0x42"),
        ("invalid", "0x42"),
        ("K", "0x42"),
        ("kiosk", "0x42"),
        ("M", "0x42"),
        ("main", "0x42"),
        ("math", "0x42"),
        ("me", "0x42"),
        ("my", "0x42"),
        ("NamedAddr", "0x42"),
        ("nfts", "0x42"),
        ("nonexistent", "0x42"),
        ("p", "0x42"),
        ("q", "0x42"),
        ("qux", "0x42"),
        ("r", "0x42"),
        ("rc", "0x42"),
        ("s", "0x42"),
        ("Self", "0x42"),
        ("serializer", "0x42"),
        ("SilverCoin", "0x42"),
        ("std", "0x42"),
        ("sui", "0x42"),
        ("Symbols", "0x42"),
        ("t1", "0x42"),
        ("t2", "0x42"),
        ("t3", "0x42"),
        ("t4", "0x42"),
        ("test", "0x42"),
        ("Test", "0x42"),
        ("test1", "0x42"),
        ("test2", "0x42"),
        ("This", "0x42"),
        ("tutorial", "0x42"),
        ("typer", "0x42"),
        ("utils", "0x42"),
        ("V0", "0x42"),
        ("V1", "0x42"),
        ("v2", "0x42"),
        ("V2", "0x42"),
        ("V3", "0x42"),
        ("V4", "0x42"),
        ("V5", "0x42"),
        ("V6", "0x42"),
        ("V7", "0x42"),
        ("V8", "0x42"),
        ("vector", "0x42"),
    ];
    mapping
        .iter()
        .map(|(name, addr)| {
            (
                name.to_string(),
                move_compiler::shared::NumericalAddress::parse_str(addr).unwrap(),
            )
        })
        .collect()
}
