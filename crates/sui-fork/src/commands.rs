// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use clap::Parser;
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::SignatureToken;
use serde_json::Value;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::SuiTypeTag;
use sui_transaction_builder::{DataReader, TransactionBuilder};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::object::Owner;
use sui_types::transaction::{Argument, CallArg, Command, TransactionDataAPI, TransactionKind};

use crate::ForkConfig;
use crate::client::ForkClient;

#[derive(Parser)]
pub enum SuiForkCommand {
    /// Start the fork RPC server (analogous to `anvil --fork-url`)
    #[clap(name = "start")]
    Start {
        /// Network to fork from: "testnet" or "mainnet"
        #[clap(long, default_value = "testnet")]
        network: String,

        /// Checkpoint sequence number to fork at (defaults to latest)
        #[clap(long)]
        checkpoint: Option<u64>,

        /// JSON-RPC server port
        #[clap(long, default_value = "9000")]
        port: u16,

        /// GraphQL URL override (overrides --network)
        #[clap(long)]
        graphql_url: Option<String>,

        /// Path to a previously dumped state file to restore on startup
        #[clap(long)]
        state: Option<PathBuf>,
    },

    /// Execute a Move function call, impersonating any sender address
    #[clap(name = "call")]
    Call {
        /// Address to impersonate as the transaction sender
        #[clap(long)]
        sender: SuiAddress,

        /// Package ID containing the function to call
        #[clap(long)]
        package: ObjectID,

        /// Module name
        #[clap(long)]
        module: String,

        /// Function name
        #[clap(long)]
        function: String,

        /// Type arguments (e.g. "0x2::sui::SUI")
        #[clap(long, num_args(0..))]
        type_args: Vec<String>,

        /// Function arguments as JSON values
        #[clap(long, num_args(0..))]
        args: Vec<SuiJsonValue>,

        /// Explicit gas coin object ID (auto-funded if not specified)
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget in MIST
        #[clap(long, default_value = "50000000")]
        gas_budget: u64,

        /// Simulate without committing state changes
        #[clap(long)]
        dry_run: bool,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Read an object from the fork and display it
    #[clap(name = "object")]
    Object {
        /// Object ID to inspect
        #[clap(long)]
        id: ObjectID,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Fund an address by minting a new Coin<SUI> directly in local state
    #[clap(name = "fund")]
    Fund {
        /// Address to fund
        #[clap(long)]
        address: SuiAddress,

        /// Amount in MIST (1 SUI = 1_000_000_000 MIST)
        #[clap(long, default_value = "10000000000")]
        amount: u64,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Seed an object from the remote network into local fork state
    #[clap(name = "seed")]
    Seed {
        /// Object ID to seed from remote
        #[clap(long)]
        object: ObjectID,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Take a state snapshot and return its ID
    #[clap(name = "snapshot")]
    Snapshot {
        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Revert state to a previously taken snapshot
    #[clap(name = "revert")]
    Revert {
        /// Snapshot ID to revert to
        #[clap(long)]
        id: u64,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Advance the chain clock by a given number of milliseconds
    #[clap(name = "advance-clock")]
    AdvanceClock {
        /// Duration to advance in milliseconds
        #[clap(long)]
        duration_ms: u64,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Set the chain clock to an absolute timestamp
    #[clap(name = "set-clock")]
    SetClock {
        /// Absolute timestamp in milliseconds since Unix epoch
        #[clap(long)]
        timestamp_ms: u64,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Advance to the next epoch
    #[clap(name = "advance-epoch")]
    AdvanceEpoch {
        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Look up a transaction by its digest
    #[clap(name = "tx")]
    Tx {
        /// Transaction digest (base58)
        #[clap(long)]
        digest: String,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Query events by transaction digest or Move event type
    #[clap(name = "events")]
    Events {
        /// Filter by transaction digest
        #[clap(long, group = "query")]
        digest: Option<String>,

        /// Filter by Move event type (e.g. "0x2::coin::CoinBalanceChanged")
        #[clap(long, group = "query")]
        event_type: Option<String>,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Display the SUI (or other coin) balance for an address
    #[clap(name = "balance")]
    Balance {
        /// Address to query
        #[clap(long)]
        address: SuiAddress,

        /// Coin type inner parameter (e.g. "0x2::sui::SUI"). Omit to show all coins.
        #[clap(long)]
        coin_type: Option<String>,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Reset the fork to its initial state (re-seeds system objects from remote)
    #[clap(name = "reset")]
    Reset {
        /// Optional checkpoint to reset to (defaults to current fork checkpoint)
        #[clap(long)]
        checkpoint: Option<u64>,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Save the current fork state to a file
    #[clap(name = "dump-state")]
    DumpState {
        /// Output file path
        #[clap(long)]
        path: PathBuf,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Load a previously saved fork state (alternative: use `sui fork start --state <path>`)
    #[clap(name = "load-state")]
    LoadState {
        /// State file to load
        #[clap(long)]
        path: PathBuf,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Call a raw JSON-RPC method on the fork server
    #[clap(name = "rpc")]
    Rpc {
        /// RPC method name (e.g. "sui_getChainIdentifier")
        method: String,

        /// JSON-encoded params array (e.g. '["0x5"]')
        #[clap(default_value = "[]")]
        params: String,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Replace an object's content in the fork with arbitrary BCS bytes
    #[clap(name = "set-object")]
    SetObject {
        /// Object ID to replace
        #[clap(long)]
        id: ObjectID,

        /// Base64-encoded BCS of the replacement Object
        #[clap(long)]
        bcs: String,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Change the owner of any object (access control testing)
    #[clap(name = "set-owner")]
    SetOwner {
        /// Object ID to change ownership of
        #[clap(long)]
        id: ObjectID,

        /// Set to AddressOwner(address)
        #[clap(long)]
        owner: Option<SuiAddress>,

        /// Set to ObjectOwner(address)
        #[clap(long = "object-owner")]
        object_owner: Option<SuiAddress>,

        /// Set to Shared
        #[clap(long)]
        shared: bool,

        /// Set to Immutable
        #[clap(long)]
        immutable: bool,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Publish a compiled Move package onto the fork
    #[clap(name = "publish")]
    Publish {
        /// Address to publish as (transaction sender)
        #[clap(long)]
        sender: SuiAddress,

        /// Path to the Move package directory (must contain build output)
        path: PathBuf,

        /// Explicit gas coin object ID (auto-funded if omitted)
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget in MIST
        #[clap(long, default_value = "100000000")]
        gas_budget: u64,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Show all locally-observed versions of an object
    #[clap(name = "history")]
    History {
        /// Object ID to inspect
        #[clap(long)]
        id: ObjectID,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// List all transactions executed on the fork
    #[clap(name = "list-tx")]
    ListTx {
        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// List dynamic fields attached to an object (e.g. Table, Bag entries)
    #[clap(name = "dynamic-fields")]
    DynamicFields {
        /// Parent object ID
        #[clap(long)]
        parent: ObjectID,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Decode a Move object's BCS contents into human-readable JSON
    #[clap(name = "decode")]
    Decode {
        /// Object ID to decode
        #[clap(long)]
        id: ObjectID,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Re-execute a transaction against current fork state
    #[clap(name = "replay")]
    Replay {
        /// Transaction digest to replay
        #[clap(long)]
        digest: String,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    // ── Bridge simulation ─────────────────────────────────────────────────────

    /// Seed the bridge object and its inner state into local fork state (prerequisite for bridge simulation)
    #[clap(name = "bridge-seed")]
    BridgeSeed {
        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Replace the on-chain bridge committee with a local test keypair (prerequisite for bridge-receive)
    #[clap(name = "bridge-setup-committee")]
    BridgeSetupCommittee {
        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Simulate an ETH→SUI bridge transfer (mints bridged tokens to `recipient`)
    #[clap(name = "bridge-receive")]
    BridgeReceive {
        /// Recipient SuiAddress
        #[clap(long)]
        recipient: SuiAddress,

        /// Token ID: 0=SUI, 1=BTC, 2=ETH, 3=USDC, 4=USDT
        #[clap(long)]
        token_id: u8,

        /// Amount in token-adjusted units
        #[clap(long)]
        amount: u64,

        /// Unique bridge action nonce (must not have been used before)
        #[clap(long)]
        nonce: u64,

        /// Source ETH chain ID (10=Mainnet, 11=Sepolia, 12=Custom)
        #[clap(long, default_value = "11")]
        eth_chain_id: u8,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },

    /// Simulate a SUI→ETH bridge transfer (emits bridge event for relaying to Ethereum)
    #[clap(name = "bridge-send")]
    BridgeSend {
        /// Sender address (must own the token coin)
        #[clap(long)]
        sender: SuiAddress,

        /// Object ID of the Coin<T> to bridge
        #[clap(long)]
        coin: ObjectID,

        /// Target ETH chain ID (10=Mainnet, 11=Sepolia, 12=Custom)
        #[clap(long, default_value = "10")]
        eth_chain_id: u8,

        /// Ethereum recipient address (hex, with or without 0x prefix)
        #[clap(long)]
        eth_recipient: String,

        /// Gas budget in MIST
        #[clap(long, default_value = "100000000")]
        gas_budget: u64,

        /// Fork server RPC URL
        #[clap(long, default_value = "http://127.0.0.1:9000")]
        rpc_url: String,
    },
}

/// Print a human-readable summary of transaction effects.
fn print_effects_summary(result: &Value) {
    let digest = result
        .get("digest")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>");
    println!("Transaction digest: {digest}");

    let Some(effects) = result.get("effects") else {
        return;
    };

    // Status
    if let Some(status) = effects.get("status") {
        let status_str = status.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        if status_str == "failure" {
            let err = status
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            println!("Status: Failure — {err}");
        } else {
            println!("Status: Success");
        }
    }

    // Gas
    if let Some(gas) = effects.get("gasUsed") {
        let computation = gas
            .get("computationCost")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        let storage = gas
            .get("storageCost")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        let rebate = gas
            .get("storageRebate")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        println!("Gas used: computation={computation}, storage={storage}, rebate={rebate}");
    }

    // Created objects
    if let Some(created) = effects.get("created").and_then(|v| v.as_array())
        && !created.is_empty()
    {
        println!("Created objects:");
        for obj in created {
            let id = obj.get("objectId").and_then(|v| v.as_str()).unwrap_or("?");
            let ver = obj.get("version").and_then(|v| v.as_str()).unwrap_or("?");
            let type_str = obj.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let owner = format_owner(obj.get("owner"));
            println!("  {id} (v{ver}) | {type_str} | {owner}");
        }
    }

    // Mutated objects
    if let Some(mutated) = effects.get("mutated").and_then(|v| v.as_array())
        && !mutated.is_empty()
    {
        println!("Mutated objects:");
        for obj in mutated {
            let id = obj.get("objectId").and_then(|v| v.as_str()).unwrap_or("?");
            let ver = obj.get("version").and_then(|v| v.as_str()).unwrap_or("?");
            let type_str = obj.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            let owner = format_owner(obj.get("owner"));
            println!("  {id} (v{ver}) | {type_str} | {owner}");
        }
    }

    // Deleted objects
    if let Some(deleted) = effects.get("deleted").and_then(|v| v.as_array())
        && !deleted.is_empty()
    {
        println!("Deleted objects:");
        for obj in deleted {
            let id = obj.get("objectId").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  {id}");
        }
    }

    // Events
    if let Some(events) = result.get("events").and_then(|v| v.as_array())
        && !events.is_empty()
    {
        println!("Events:");
        for (i, event) in events.iter().enumerate() {
            let type_str = event.get("type").and_then(|v| v.as_str()).unwrap_or("?");
            println!("  [{i}] {type_str}");
        }
    }
}

fn format_owner(owner: Option<&Value>) -> String {
    let Some(owner) = owner else {
        return "?".to_string();
    };
    if let Some(addr) = owner.get("AddressOwner").and_then(|v| v.as_str()) {
        return format!("AddressOwner({addr})");
    }
    if let Some(obj) = owner.get("ObjectOwner").and_then(|v| v.as_str()) {
        return format!("ObjectOwner({obj})");
    }
    if owner.get("Shared").is_some() {
        return "Shared".to_string();
    }
    if owner.as_str() == Some("Immutable") {
        return "Immutable".to_string();
    }
    owner.to_string()
}

impl SuiForkCommand {
    pub async fn execute(self) -> Result<()> {
        match self {
            SuiForkCommand::Start {
                network,
                checkpoint,
                port,
                graphql_url,
                state,
            } => {
                let node = match graphql_url {
                    Some(url) => {
                        if checkpoint.is_none() && state.is_none() {
                            return Err(anyhow::anyhow!(
                                "--checkpoint is required when using --graphql-url without --state"
                            ));
                        }
                        sui_data_store::node::Node::Custom(url)
                    }
                    None => match network.as_str() {
                        "mainnet" => sui_data_store::node::Node::Mainnet,
                        _ => sui_data_store::node::Node::Testnet,
                    },
                };
                crate::run(ForkConfig {
                    node,
                    checkpoint,
                    rpc_port: port,
                    state_file: state,
                })
                .await
            }

            SuiForkCommand::Call {
                sender,
                package,
                module,
                function,
                type_args,
                args,
                gas,
                gas_budget,
                dry_run,
                rpc_url,
            } => {
                let client = Arc::new(ForkClient::new(rpc_url.clone()));
                let builder = TransactionBuilder::new(client.clone());

                // Auto-fund gas when no explicit coin is provided.
                // This creates a fresh gas coin for the sender on the fork.
                let gas_coin = match gas {
                    Some(id) => Some(id),
                    None => {
                        let coin_id = client
                            .fund_account(sender, gas_budget.saturating_mul(2))
                            .await?;
                        if !dry_run {
                            println!("Auto-funded gas coin: {coin_id}");
                        }
                        Some(coin_id)
                    }
                };

                let type_args: Vec<SuiTypeTag> =
                    type_args.into_iter().map(SuiTypeTag::new).collect();

                let mut tx_data = builder
                    .move_call(
                        sender,
                        package,
                        &module,
                        &function,
                        type_args,
                        args,
                        gas_coin,
                        gas_budget,
                        None,
                    )
                    .await?;

                // Inspect Move function return types. Any returned object (a struct
                // with the `key` ability) that also lacks `drop` would cause
                // UnusedValueWithoutDrop if left unconsumed. We add a
                // TransferObjects command to send all object returns to the sender,
                // which is safe even for types that do have `drop`.
                let pkg_obj = client.as_ref().get_object(package).await
                    .map_err(|e| anyhow!("failed to fetch package {package}: {e}"))?;
                if let Some(move_pkg) = pkg_obj.data.try_as_package()
                    && let Ok(compiled_mod) =
                        move_pkg.deserialize_module_by_str(&module, &BinaryConfig::standard())
                {
                    let function_ident =
                        move_core_types::identifier::Identifier::new(&*function)
                            .map_err(|e| anyhow!("invalid function name '{function}': {e}"))?;

                    let return_sig = compiled_mod
                        .function_defs
                        .iter()
                        .find(|fdef| {
                            compiled_mod.identifier_at(
                                compiled_mod.function_handle_at(fdef.function).name,
                            ) == function_ident.as_ident_str()
                        })
                        .map(|fdef| {
                            let handle = compiled_mod.function_handle_at(fdef.function);
                            compiled_mod.signature_at(handle.return_).0.clone()
                        })
                        .unwrap_or_default();

                    // Collect indices of object-typed returns (structs with `key`).
                    let object_return_indices: Vec<u16> = return_sig
                        .iter()
                        .enumerate()
                        .filter_map(|(i, token)| {
                            let dtype_idx = match token {
                                SignatureToken::Datatype(idx) => *idx,
                                SignatureToken::DatatypeInstantiation(inner) => inner.0,
                                _ => return None,
                            };
                            compiled_mod
                                .datatype_handle_at(dtype_idx)
                                .abilities
                                .has_key()
                                .then_some(i as u16)
                        })
                        .collect();

                    if !object_return_indices.is_empty() {
                        let kind = tx_data.kind_mut();
                        if let TransactionKind::ProgrammableTransaction(ptb) = kind {
                            let sender_bytes = bcs::to_bytes(&sender).map_err(|e| {
                                anyhow!("failed to serialize sender: {e}")
                            })?;
                            ptb.inputs.push(CallArg::Pure(sender_bytes));
                            let sender_input_idx = (ptb.inputs.len() - 1) as u16;
                            let object_args: Vec<Argument> = object_return_indices
                                .iter()
                                // The move call is always command 0 in the PTB built
                                // by TransactionBuilder::move_call.
                                .map(|&i| Argument::NestedResult(0, i))
                                .collect();
                            ptb.commands.push(Command::TransferObjects(
                                object_args,
                                Argument::Input(sender_input_idx),
                            ));
                        }
                    }
                }

                let tx_bytes = bcs::to_bytes(&tx_data)?;
                let tx_b64 = BASE64_STANDARD.encode(&tx_bytes);

                let result = if dry_run {
                    client.dry_run_transaction(tx_b64).await?
                } else {
                    client.execute_transaction(tx_b64).await?
                };

                if dry_run {
                    println!("[Dry run — state not committed]");
                }
                print_effects_summary(&result);
                Ok(())
            }

            SuiForkCommand::Object { id, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.get_object_json(id).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }

            SuiForkCommand::Fund {
                address,
                amount,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                let coin_id = client.fund_account(address, amount).await?;
                println!("Funded {address} with {amount} MIST");
                println!("Coin object ID: {coin_id}");
                Ok(())
            }

            SuiForkCommand::Seed { object, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let found = client.fork_seed_object(object).await?;
                if found {
                    println!("Seeded object {object} from remote network");
                } else {
                    println!("Object {object} not found at fork checkpoint — nothing seeded");
                }
                Ok(())
            }

            SuiForkCommand::Snapshot { rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let id = client.fork_snapshot().await?;
                println!("Snapshot ID: {id}");
                Ok(())
            }

            SuiForkCommand::Revert { id, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                client.fork_revert(id).await?;
                println!("Reverted to snapshot {id}");
                Ok(())
            }

            SuiForkCommand::AdvanceClock {
                duration_ms,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                client.fork_advance_clock(duration_ms).await?;
                println!("Clock advanced by {duration_ms} ms");
                Ok(())
            }

            SuiForkCommand::SetClock {
                timestamp_ms,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                client.fork_set_clock_timestamp(timestamp_ms).await?;
                println!("Clock set to {timestamp_ms} ms");
                Ok(())
            }

            SuiForkCommand::AdvanceEpoch { rpc_url } => {
                let client = ForkClient::new(rpc_url);
                client.fork_advance_epoch().await?;
                println!("Advanced to next epoch");
                Ok(())
            }

            SuiForkCommand::Tx { digest, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.get_transaction_block(&digest).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }

            SuiForkCommand::Events {
                digest,
                event_type,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                let result = if let Some(ref d) = digest {
                    client.query_events_by_tx(d).await?
                } else if let Some(ref et) = event_type {
                    client.query_events_by_type(et).await?
                } else {
                    return Err(anyhow!("one of --digest or --event-type is required"));
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }

            SuiForkCommand::Balance {
                address,
                coin_type,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                if coin_type.is_some() {
                    let result = client.get_balance(address, coin_type.as_deref()).await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    let result = client.get_all_balances(address).await?;
                    let balances = result.as_array().cloned().unwrap_or_default();
                    if balances.is_empty() {
                        println!("No coins in local state for {address}");
                        println!(
                            "Note: only objects already seeded into the fork are visible here."
                        );
                    } else {
                        for bal in &balances {
                            let coin_type = bal
                                .get("coinType")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            let total = bal
                                .get("totalBalance")
                                .and_then(|v| v.as_str())
                                .unwrap_or("0");
                            let count = bal
                                .get("coinObjectCount")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            println!("{coin_type}: {total} MIST ({count} objects)");
                        }
                    }
                }
                Ok(())
            }

            SuiForkCommand::Reset { checkpoint, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                client.fork_reset(checkpoint).await?;
                println!("Fork reset to initial state");
                Ok(())
            }

            SuiForkCommand::DumpState { path, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let path_str = path
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid path"))?
                    .to_string();
                client.fork_dump_state(&path_str).await?;
                println!("State saved to {}", path.display());
                Ok(())
            }

            SuiForkCommand::LoadState { path, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let path_str = path
                    .to_str()
                    .ok_or_else(|| anyhow!("invalid path"))?
                    .to_string();
                client.fork_load_state(&path_str).await?;
                println!("State loaded from {}", path.display());
                Ok(())
            }

            SuiForkCommand::Rpc {
                method,
                params,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                let params_val: Value = serde_json::from_str(&params)
                    .map_err(|e| anyhow!("invalid JSON params: {e}"))?;
                let result = client.call(&method, params_val).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }

            SuiForkCommand::SetObject { id, bcs, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                client.fork_set_object_bcs(id, bcs).await?;
                println!("Object {id} updated");
                Ok(())
            }

            SuiForkCommand::SetOwner {
                id,
                owner,
                object_owner,
                shared,
                immutable,
                rpc_url,
            } => {
                let new_owner = if let Some(addr) = owner {
                    Owner::AddressOwner(addr)
                } else if let Some(addr) = object_owner {
                    Owner::ObjectOwner(addr)
                } else if shared {
                    Owner::Shared {
                        initial_shared_version: SequenceNumber::from(1),
                    }
                } else if immutable {
                    Owner::Immutable
                } else {
                    return Err(anyhow!(
                        "one of --owner, --object-owner, --shared, or --immutable is required"
                    ));
                };
                let owner_json = serde_json::to_value(&new_owner)?;
                let client = ForkClient::new(rpc_url);
                client.fork_set_owner(id, owner_json).await?;
                println!("Owner of {id} updated");
                Ok(())
            }

            SuiForkCommand::Publish {
                sender,
                path,
                gas,
                gas_budget,
                rpc_url,
            } => {
                let modules = find_mv_files(&path)?;
                if modules.is_empty() {
                    return Err(anyhow!(
                        "no compiled .mv files found under {}. Did you run `sui move build`?",
                        path.display()
                    ));
                }
                println!("Found {} compiled module(s)", modules.len());

                let client = Arc::new(ForkClient::new(rpc_url.clone()));
                let builder = TransactionBuilder::new(client.clone());

                let gas_coin = match gas {
                    Some(id) => Some(id),
                    None => {
                        let coin_id = client
                            .fund_account(sender, gas_budget.saturating_mul(2))
                            .await?;
                        println!("Auto-funded gas coin: {coin_id}");
                        Some(coin_id)
                    }
                };

                // Standard Sui framework dependency IDs: MoveStdlib, SuiFramework, SuiSystem
                let dep_ids: Vec<ObjectID> = vec![
                    "0x0000000000000000000000000000000000000000000000000000000000000001"
                        .parse()?,
                    "0x0000000000000000000000000000000000000000000000000000000000000002"
                        .parse()?,
                    "0x0000000000000000000000000000000000000000000000000000000000000003"
                        .parse()?,
                ];

                let tx_data = builder
                    .publish(sender, modules, dep_ids, gas_coin, gas_budget)
                    .await?;
                let tx_bytes = bcs::to_bytes(&tx_data)?;
                let tx_b64 = BASE64_STANDARD.encode(&tx_bytes);
                let result = client.execute_transaction(tx_b64).await?;
                print_effects_summary(&result);

                // Print the created package ID (package objects have an empty type string)
                if let Some(created) = result
                    .get("effects")
                    .and_then(|e| e.get("created"))
                    .and_then(|c| c.as_array())
                {
                    for obj in created {
                        if obj.get("type").and_then(|v| v.as_str()) == Some("") {
                            let id = obj.get("objectId").and_then(|v| v.as_str()).unwrap_or("?");
                            println!("Package ID: {id}");
                        }
                    }
                }
                Ok(())
            }

            SuiForkCommand::History { id, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.fork_get_object_history(id).await?;
                let versions = result.as_array().cloned().unwrap_or_default();
                if versions.is_empty() {
                    println!("No history found for {id} in local state");
                } else {
                    println!("{} version(s) of {id}:", versions.len());
                    for v in &versions {
                        let ver = v
                            .get("data")
                            .and_then(|d| d.get("version"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        let digest = v
                            .get("data")
                            .and_then(|d| d.get("digest"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        println!("  v{ver} — {digest}");
                    }
                }
                Ok(())
            }

            SuiForkCommand::ListTx { rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.fork_list_transactions().await?;
                let txs = result.as_array().cloned().unwrap_or_default();
                if txs.is_empty() {
                    println!("No transactions executed on this fork yet");
                } else {
                    println!("{} transaction(s):", txs.len());
                    for tx in &txs {
                        let digest = tx.get("digest").and_then(|v| v.as_str()).unwrap_or("?");
                        let status = tx.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                        let gas = tx.get("gasUsed").and_then(|v| v.as_str()).unwrap_or("0");
                        println!("  {digest} | {status} | gas={gas}");
                    }
                }
                Ok(())
            }

            SuiForkCommand::DynamicFields { parent, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.fork_get_dynamic_fields(parent).await?;
                let fields = result
                    .get("data")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                if fields.is_empty() {
                    println!(
                        "No locally-cached dynamic fields found for {parent}.\n\
                         Note: only objects already seeded into the fork are visible."
                    );
                } else {
                    println!("{} dynamic field(s) for {parent}:", fields.len());
                    for f in &fields {
                        let id = f.get("objectId").and_then(|v| v.as_str()).unwrap_or("?");
                        let ver = f.get("version").and_then(|v| v.as_str()).unwrap_or("?");
                        let type_str = f.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                        println!("  {id} (v{ver}) | {type_str}");
                    }
                }
                Ok(())
            }

            SuiForkCommand::Decode { id, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.fork_decode_object(id).await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }

            SuiForkCommand::Replay { digest, rpc_url } => {
                let client = ForkClient::new(rpc_url);
                let result = client.fork_replay_transaction(&digest).await?;
                println!("[Replayed in current fork state]");
                print_effects_summary(&result);
                Ok(())
            }

            SuiForkCommand::BridgeSeed { rpc_url } => {
                let client = ForkClient::new(rpc_url);
                client.fork_seed_bridge_objects().await?;
                println!("Bridge objects seeded into local fork state");
                Ok(())
            }

            SuiForkCommand::BridgeSetupCommittee { rpc_url } => {
                let client = ForkClient::new(rpc_url);
                client.fork_setup_bridge_test_committee().await?;
                println!("Bridge committee replaced with local test keypair");
                println!("You can now use `sui fork bridge-receive` to simulate ETH→SUI transfers");
                Ok(())
            }

            SuiForkCommand::BridgeReceive {
                recipient,
                token_id,
                amount,
                nonce,
                eth_chain_id,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                let result = client
                    .fork_simulate_eth_to_sui_bridge(
                        recipient, token_id, amount, nonce, eth_chain_id,
                    )
                    .await?;
                println!("[ETH→SUI bridge simulation]");
                print_effects_summary(&result);
                Ok(())
            }

            SuiForkCommand::BridgeSend {
                sender,
                coin,
                eth_chain_id,
                eth_recipient,
                gas_budget,
                rpc_url,
            } => {
                let client = ForkClient::new(rpc_url);
                let result = client
                    .fork_simulate_sui_to_eth_bridge(
                        sender, coin, eth_chain_id, &eth_recipient, gas_budget,
                    )
                    .await?;
                println!("[SUI→ETH bridge simulation]");
                print_effects_summary(&result);
                if let Some(events) = result.get("events").and_then(|v| v.as_array()) {
                    for event in events {
                        let type_str =
                            event.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                        if type_str.contains("TokenBridgeEvent")
                            || type_str.contains("TokenTransferInitiated")
                        {
                            println!("Bridge event emitted: {type_str}");
                        }
                    }
                }
                Ok(())
            }
        }
    }
}

/// Recursively collect all compiled `.mv` bytecode files under `dir`.
fn find_mv_files(dir: &Path) -> Result<Vec<Vec<u8>>> {
    let mut modules = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|e| anyhow!("cannot read directory {}: {e}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            modules.extend(find_mv_files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "mv") {
            modules.push(
                std::fs::read(&path)
                    .map_err(|e| anyhow!("failed to read {}: {e}", path.display()))?,
            );
        }
    }
    Ok(modules)
}
