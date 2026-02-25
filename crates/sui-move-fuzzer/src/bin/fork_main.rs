// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Forked-execution mode entry point.
//!
//! Runs the fuzzer's module-generation and mutation engine against real Sui
//! network state fetched lazily from an RPC node.  Oracle price objects can be
//! overridden so that adversarial modules see manipulated prices.
//!
//! # Usage
//!
//! ```text
//! cargo run --features fork --bin fork_main -- \
//!     --fork-rpc https://fullnode.mainnet.sui.io:443 \
//!     --oracle-override 0x<price_feed_id>:500000000 \
//!     --chain mainnet \
//!     -c ./corpus \
//!     -d ./findings \
//!     -p 20
//! ```
//!
//! # Flags
//!
//! | Flag | Default | Description |
//! |------|---------|-------------|
//! | `--fork-rpc URL` | (required) | RPC endpoint to fetch live chain state from |
//! | `--chain mainnet\|testnet` | `mainnet` | Network chain ID for protocol config |
//! | `--oracle-override ID:PRICE` | none | Override an oracle price object (repeatable) |
//! | `-c DIR` | `./corpus` | Corpus directory for generated inputs |
//! | `-d DIR` | `./findings` | Output directory for bug reports |
//! | `-p N` | `100` | Number of iterations to run |
//! | `--seed N` | random | RNG seed for reproducible runs |
//! | `-v` | off | Verbose output |

use std::path::PathBuf;

use rand::SeedableRng;
use sui_move_fuzzer::{
    forked_executor::ForkedExecutor,
    module_gen::{ModuleBuilder, ModuleGenConfig},
    oracle,
    oracle_override::{apply_price_override, build_price_patches, parse_override_spec, PriceOverride},
};
use sui_protocol_config::Chain;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::MOVE_STDLIB_PACKAGE_ID;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;

fn main() {
    let config = match Config::from_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("{}", USAGE);
            std::process::exit(1);
        }
    };

    if let Err(e) = run(config) {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

fn run(config: Config) -> anyhow::Result<()> {
    eprintln!(
        "[fork_main] connecting to {} (chain={:?})",
        config.fork_rpc, config.chain
    );

    let mut exec = ForkedExecutor::new(&config.fork_rpc, config.chain)?;

    // Apply oracle price overrides.
    for (oracle_id, price) in &config.oracle_overrides {
        if config.verbose {
            eprintln!("[fork_main] override oracle {oracle_id} → price={price}");
        }
        let spec = PriceOverride {
            oracle_object_id: *oracle_id,
            price: *price,
            confidence: None,
            exponent: None,
        };
        let patches = build_price_patches(&spec);
        apply_price_override(exec.store_mut(), &spec, &patches)?;
    }

    std::fs::create_dir_all(&config.corpus_dir)?;
    std::fs::create_dir_all(&config.findings_dir)?;

    // Seed the RNG — print it so any run can be reproduced with `--seed`.
    let seed = config
        .seed
        .unwrap_or_else(|| rand::Rng::r#gen(&mut rand::thread_rng()));
    eprintln!("[fork_main] seed={seed}");
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let mut bugs_found = 0usize;
    let dep_ids = vec![MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID];

    eprintln!("[fork_main] running {} iterations", config.iterations);

    for iter in 0..config.iterations {
        // Generate a module using the grammar-based builder with random config.
        let gen_config = random_gen_config(&mut rng);
        // Use a local Vec<u8> to avoid leaking memory on every iteration.
        let entropy: Vec<u8> =
            (0..1024).map(|_| rand::Rng::r#gen::<u8>(&mut rng)).collect();
        let mut u = arbitrary::Unstructured::new(&entropy);
        let module = match ModuleBuilder::new(gen_config.clone()).build(&mut u) {
            Ok(m) => m,
            Err(_) => continue, // arbitrary ran out of data — skip
        };

        // Extract the module name before serializing so we can call entry fns.
        let self_handle =
            &module.module_handles[module.self_module_handle_idx.0 as usize];
        let module_name =
            module.identifiers[self_handle.name.0 as usize].as_str().to_string();

        let mut module_bytes = Vec::new();
        if module.serialize(&mut module_bytes).is_err() {
            continue;
        }

        // Call the first entry function (named "f0") when the generated module
        // has one — this surfaces runtime bugs that only appear during execution.
        let function_calls: Vec<(&str, &str)> = if gen_config.has_entry_fn {
            vec![(module_name.as_str(), "f0")]
        } else {
            vec![]
        };

        // Wrap execution in catch_unwind to catch validator panics.
        let call_result = oracle::check_crash("fork_exec", || {
            exec.publish_and_call(module_bytes.clone(), dep_ids.clone(), &function_calls)
        });

        match call_result {
            Ok(Ok(result)) => {
                if config.verbose {
                    eprintln!("[iter {iter}] status: {:?}", result.effects.status());
                }
                // Delegate invariant-violation detection to the shared oracle.
                if let Some(bug) = oracle::check_effects_for_bugs(&result.effects) {
                    bugs_found += 1;
                    let finding_path = config
                        .findings_dir
                        .join(format!("fork-invariant-{iter}.bin"));
                    std::fs::write(&finding_path, &module_bytes)?;
                    eprintln!(
                        "[fork_main] BUG FOUND (iter {iter}): {:?} — saved to {}",
                        bug,
                        finding_path.display()
                    );
                }
            }
            Ok(Err(e)) => {
                if config.verbose {
                    eprintln!("[iter {iter}] exec error: {e}");
                }
            }
            Err(bug) => {
                // Executor panicked — save the triggering module as a finding.
                bugs_found += 1;
                let finding_path =
                    config.findings_dir.join(format!("fork-panic-{iter}.bin"));
                std::fs::write(&finding_path, &module_bytes)?;
                eprintln!(
                    "[fork_main] PANIC (iter {iter}): {:?} — saved to {}",
                    bug,
                    finding_path.display()
                );
            }
        }
    }

    eprintln!(
        "[fork_main] done — {} iterations, {} bugs found",
        config.iterations, bugs_found
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Config and arg parsing
// ---------------------------------------------------------------------------

struct Config {
    fork_rpc: String,
    chain: Chain,
    oracle_overrides: Vec<(sui_types::base_types::ObjectID, i64)>,
    corpus_dir: PathBuf,
    findings_dir: PathBuf,
    iterations: usize,
    seed: Option<u64>,
    verbose: bool,
}

const USAGE: &str = "\
Usage: fork_main [OPTIONS]

Required:
  --fork-rpc URL              RPC endpoint for live chain state

Optional:
  --chain mainnet|testnet     Chain (default: mainnet)
  --oracle-override ID:PRICE  Override oracle price, repeatable
  -c DIR                      Corpus directory (default: ./corpus)
  -d DIR                      Findings directory (default: ./findings)
  -p N                        Number of iterations (default: 100)
  --seed N                    RNG seed for reproducible runs (default: random)
  -v                          Verbose output
  -h, --help                  Show this message";

impl Config {
    fn from_args() -> anyhow::Result<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut fork_rpc: Option<String> = None;
        let mut chain = Chain::Mainnet;
        let mut oracle_overrides = Vec::new();
        let mut corpus_dir = PathBuf::from("./corpus");
        let mut findings_dir = PathBuf::from("./findings");
        let mut iterations: usize = 100;
        let mut seed: Option<u64> = None;
        let mut verbose = false;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--fork-rpc" => {
                    i += 1;
                    fork_rpc = Some(next_arg(&args, i, "--fork-rpc")?);
                }
                "--chain" => {
                    i += 1;
                    let s = next_arg(&args, i, "--chain")?;
                    chain = match s.as_str() {
                        "mainnet" => Chain::Mainnet,
                        "testnet" => Chain::Testnet,
                        other => anyhow::bail!("unknown chain '{other}', use mainnet or testnet"),
                    };
                }
                "--oracle-override" => {
                    i += 1;
                    let s = next_arg(&args, i, "--oracle-override")?;
                    oracle_overrides.push(parse_override_spec(&s)?);
                }
                "-c" => {
                    i += 1;
                    corpus_dir = PathBuf::from(next_arg(&args, i, "-c")?);
                }
                "-d" => {
                    i += 1;
                    findings_dir = PathBuf::from(next_arg(&args, i, "-d")?);
                }
                "-p" => {
                    i += 1;
                    let s = next_arg(&args, i, "-p")?;
                    iterations = s
                        .parse()
                        .map_err(|_| anyhow::anyhow!("-p requires a positive integer, got '{s}'"))?;
                }
                "--seed" => {
                    i += 1;
                    let s = next_arg(&args, i, "--seed")?;
                    seed = Some(
                        s.parse()
                            .map_err(|_| anyhow::anyhow!("--seed requires a u64, got '{s}'"))?,
                    );
                }
                "-v" => verbose = true,
                "-h" | "--help" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                unknown => anyhow::bail!("unknown flag '{unknown}'\n{USAGE}"),
            }
            i += 1;
        }

        let fork_rpc = fork_rpc.ok_or_else(|| anyhow::anyhow!("--fork-rpc is required"))?;

        Ok(Config {
            fork_rpc,
            chain,
            oracle_overrides,
            corpus_dir,
            findings_dir,
            iterations,
            seed,
            verbose,
        })
    }
}

fn next_arg(args: &[String], i: usize, flag: &str) -> anyhow::Result<String> {
    args.get(i)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} requires an argument"))
}

fn random_gen_config(rng: &mut impl rand::Rng) -> ModuleGenConfig {
    ModuleGenConfig {
        num_structs: rng.gen_range(0..=6),
        num_functions: rng.gen_range(1..=6),
        num_fields_per: rng.gen_range(0..=4),
        max_code_len: rng.gen_range(4..=48),
        has_key_struct: rand::Rng::r#gen::<bool>(rng),
        has_entry_fn: rand::Rng::r#gen::<bool>(rng),
    }
}
