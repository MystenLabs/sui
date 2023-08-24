// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::time::{Duration, Instant};

use clap::Parser;
use futures::StreamExt;
use petgraph::graph::Graph;
use petgraph::algo::kosaraju_scc;
use petgraph::prelude::*;
use sui_sdk::{SuiClientBuilder, SuiClient};
use sui_sdk::rpc_types::{
    SuiCallArg, SuiObjectArg, SuiObjectRef, SuiTransactionBlockKind,
    SuiTransactionBlockData, SuiTransactionBlockResponseOptions,
    SuiTransactionBlockResponseQuery, SuiTransactionBlockEffects,
    SuiObjectDataOptions, SuiCommand, SuiTransactionBlockResponse,
};
use sui_sdk::types::base_types::{ObjectID, SequenceNumber};
use sui_sdk::types::digests::TransactionDigest;

type WUUGraph = Graph<u64, (), Undirected>;
type ObjRef = (ObjectID, SequenceNumber);

#[derive(PartialEq)]
enum Mode {
    Scheduling, // outputs the graph metrics, including total gas, max CC, schedule
    Graph,      // outputs the graph nodes and edges, to be parsed by cliques.py
    Accesses,   // outputs the accesses per tx split by type
}

#[derive(Default)]
struct AccessesData {
    tx_digest: TransactionDigest,
    programmable: bool,

    reads: HashSet<ObjRef>,
    writes: HashSet<ObjRef>,

    count_gas: usize,
    count_imm: usize,
    count_own: usize,
    count_cre: usize,
    count_shr: usize,
    count_shw: usize,
    count_oth: usize,
}

impl AccessesData {
    fn new(tx_digest: TransactionDigest) -> Self {
        Self {
            tx_digest,
            ..Self::default()
        }
    }
}

#[derive(Default)]
struct BatchData {
    batch: u64,
    epoch: u64,
    working_sets: Vec<(HashSet<ObjRef>, HashSet<ObjRef>)>,
    txs_gas: Vec<u64>,
    txs_times: Vec<u64>,
}

impl BatchData {
    fn to_graph(&mut self) {
        if MODE == Mode::Graph || MODE == Mode::Scheduling {
            let mut conflicts = HashSet::new();
            for (idx1, (_, writes1)) in self.working_sets.iter().enumerate() {
                for (idx2, (reads2, writes2)) in self.working_sets.iter().enumerate().skip(idx1 + 1) {
                    for w in writes1 {
                        if writes2.contains(w) || reads2.contains(w) {
                            conflicts.insert((idx1, idx2));
                        }
                    }
                }
            }

            let mut graph: WUUGraph = Graph::new_undirected();
            let nodes: Vec<_> = self.txs_gas.iter().map(|g| graph.add_node(*g)).collect();
            if MODE == Mode::Graph {
                println!("batch {} epoch {}", self.batch, self.epoch);
                for (i, g) in self.txs_gas.iter().enumerate() {
                    println!("n {} {}", i, g);
                }
            }
            for &(a, b) in &conflicts {
                graph.add_edge(nodes[a], nodes[b], ());

                if MODE == Mode::Graph {
                    println!("e {} {}", a, b);
                }
            }

            if MODE == Mode::Scheduling {
                let num_nodes = graph.node_count();
                let max_node_weight = graph.node_weights().max().unwrap_or(&0);
                let mut max_cc_weight = 0;
                for cc in kosaraju_scc(&graph) {
                    let weight_sum: u64 = cc.iter().map(|&n| graph.node_weight(n).unwrap()).sum();
                    max_cc_weight = std::cmp::max(max_cc_weight, weight_sum);
                }

                let (total_gas, sequential_gas) = list_scheduling(&self.txs_gas, &conflicts);

                println!("{},{},{},{},{},{},{},{}",
                    self.batch,
                    self.epoch,
                    num_nodes,
                    conflicts.len(),
                    total_gas,
                    sequential_gas,
                    max_cc_weight,
                    max_node_weight
                );
            }
        }

        self.batch += 1;
        self.reset();
    }

    fn reset(&mut self) {
        self.epoch = 0;
        self.working_sets.clear();
        self.txs_gas.clear();
        self.txs_times.clear();
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Directory for storing the data (default: ./data)
    #[arg(long, value_name = "DIR")]
    data_dir: Option<String>,

    /// Number of txs to batch together (default: 150)
    #[arg(long, value_name = "NUM TX")]
    batch_size: Option<usize>,

    /// URL of the Sui client RPC API (default: http://localhost:9000)
    #[arg(long, value_name = "URL")]
    client_url: Option<String>,

    /// Epoch to start from (default: 0)
    #[arg(long, value_name = "EPOCH")]
    from_epoch: Option<u64>,

    /// Last epoch to process (default: most recent full epoch)
    #[arg(long, value_name = "EPOCH")]
    to_epoch: Option<u64>,

    /// Digest of the last tx already processed (default: None)
    #[arg(long, value_name = "TX DIGEST")]
    offset: Option<String>,
}

// Default Configuration
const DEFAULT_DATA_DIR: &str = "./data";
const DEFAULT_BATCH_SIZE: usize = 150;
const DEFAULT_CLIENT_URL: &str = "http://localhost:9000";
const MODE: Mode = Mode::Scheduling;

const OPTIONS: SuiTransactionBlockResponseOptions = SuiTransactionBlockResponseOptions {
    show_input: true,
    show_raw_input: false,
    show_effects: true,
    show_events: false,
    show_object_changes: false,
    show_balance_changes: false,
};
const TX_QUERY: SuiTransactionBlockResponseQuery = SuiTransactionBlockResponseQuery {
    filter: None,
    options: Some(OPTIONS),
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    let sui = SuiClientBuilder::default().build(cli.client_url.unwrap_or(DEFAULT_CLIENT_URL.to_string())).await.unwrap();

    let to_epoch = cli.to_epoch.unwrap_or(20);

    let mut tx_digest = cli.offset.map(|s| s.parse().unwrap());
    let mut total_txs = sui.read_api().get_total_transaction_blocks().await?;
    let mut cp_txs = Vec::new();

    // Otherwise: Use binary search to find the tx offset for the epoch boundary.
    if tx_digest.is_none() {
        if let Some(from_epoch) = cli.from_epoch {
            let mut left = 0;
            let mut right = sui.read_api().get_latest_checkpoint_sequence_number().await?;
            let mut cursor = ((left + right) / 2).into();
            'outer: loop {
                let page = sui.read_api().get_checkpoints(Some(cursor), None, false).await?;
                eprintln!("{:?}-{:?}", page.data.first().unwrap().epoch, page.data.last().unwrap().epoch);
                if page.data.first().unwrap().epoch >= from_epoch {
                    right = *cursor;
                } else if page.data.last().unwrap().epoch < from_epoch {
                    left = *cursor;
                } else {
                    for cp in page.data {
                        if cp.epoch == from_epoch {
                            break 'outer;
                        }
                        tx_digest = Some(*cp.transactions.last().unwrap());
                    }
                }
                cursor = ((left + right) / 2).into();
            }

            let mut page = sui.read_api().get_checkpoints(Some(cursor), None, false).await?;
            total_txs = 0;
            while page.has_next_page && page.data.last().unwrap().epoch <= to_epoch && cp_txs.len() < 1000 {
                for cp in page.data {
                    if cp.epoch >= from_epoch && cp.epoch <= to_epoch {
                        total_txs += cp.transactions.len() as u64;
                        cp_txs.push(cp.transactions);
                    }
                }
                cursor = page.next_cursor.unwrap();
                page = sui.read_api().get_checkpoints(Some(cursor), None, false).await?;
            }

            eprintln!("Epochs {} through {}: a total of {} txs", from_epoch, to_epoch, total_txs);
        }
    }


    //let tx_stream = sui.read_api().get_transactions_stream(TX_QUERY, tx_digest, false);
    //futures::pin_mut!(tx_stream);
    let mut ctr = 0;
    let mut batch_data = BatchData::default();

    if MODE == Mode::Scheduling {
        //println!("batch,epoch,num_nodes,conflicts,total_gas,sequential_gas,max_cc_gas,max_tx_gas");
    } else if MODE == Mode::Accesses {
        //println!("tx_digest,epoch,gas,immutable,owned,created,shared_read,shared_write,other");
    }

    let start = Instant::now();

    for this_cp_txs in cp_txs {
        for digest in this_cp_txs {
            let tx = sui.read_api().get_transaction_with_options(digest, OPTIONS).await;
            if tx.is_err() {
                eprintln!("ERR processing tx {} failed", ctr);
                continue;
            }
            let tx = tx.unwrap();
            let SuiTransactionBlockEffects::V1(effects) = tx.effects.as_ref().unwrap();
            let gas_used = effects.gas_used.computation_cost;
            let epoch = effects.executed_epoch;
            let timestamp = tx.timestamp_ms.expect("Found tx w/o timestamp!");

            /*if !batch_data.working_sets.is_empty() && batch_data.epoch != epoch {
                batch_data.to_graph();
            }*/

            if epoch > to_epoch {
                eprintln!("{} > {}", epoch, to_epoch);
                break;
            }

            let accesses = parse_access_data_from_tx(&tx, &sui).await?;

            if MODE == Mode::Accesses && accesses.programmable {
                println!("{},{},{},{},{},{},{},{},{}",
                    accesses.tx_digest,
                    epoch,
                    accesses.count_gas,
                    accesses.count_imm,
                    accesses.count_own,
                    accesses.count_cre,
                    accesses.count_shr,
                    accesses.count_shw,
                    accesses.count_oth,
                );
            }

            if accesses.reads.len() > 0 || accesses.writes.len() > 0 {
                batch_data.working_sets.push((accesses.reads, accesses.writes));
                batch_data.txs_gas.push(gas_used);
                batch_data.txs_times.push(timestamp);
                batch_data.epoch = epoch;
            }

            ctr += 1;
            if ctr % 10_000 == 0 {
                print_progress(ctr, total_txs, &start);
            }

            /*if batch_data.working_sets.len() == cli.batch_size.unwrap_or(DEFAULT_BATCH_SIZE) {
                batch_data.to_graph();
            }*/
        }

        batch_data.to_graph();
    }

    Ok(())
}

async fn parse_access_data_from_tx(tx: &SuiTransactionBlockResponse, sui: &SuiClient) -> Result<AccessesData, anyhow::Error> {
    let SuiTransactionBlockEffects::V1(effects) = tx.effects.as_ref().unwrap();
    let digest = tx.digest;
    let tx = tx.transaction.as_ref().unwrap();
    let SuiTransactionBlockData::V1(data) = &tx.data;
    let mut accesses = AccessesData::new(digest);

    for obj_ref in &effects.created {
        let SuiObjectRef {object_id, version, ..} = obj_ref.reference;
        accesses.writes.insert((object_id, version));
        accesses.count_cre += 1;
    }

    let gas = &data.gas_data;
    let gas_objects: Vec<_> = gas.payment.iter().map(|obj| obj).collect();
    if let SuiTransactionBlockKind::ProgrammableTransaction(tx) = &data.transaction {
        accesses.programmable = true;

        for gas_obj in gas_objects {
            accesses.writes.insert((gas_obj.object_id, gas_obj.version));
            accesses.count_gas += 1;
        }

        for input in &tx.inputs {
            if let SuiCallArg::Object(obj) = input {
                if let &SuiObjectArg::ImmOrOwnedObject { object_id, version, .. } = obj {
                    let opt = SuiObjectDataOptions::new().with_owner();
                    let obj = sui.read_api().get_object_with_options(object_id, opt).await?;
                    if let Some(data) = obj.data {
                        if data.owner.expect("ImmOrOwnedObject has no owner").is_immutable() {
                            accesses.reads.insert((object_id, 0.into()));
                            accesses.count_imm += 1;
                        } else {
                            accesses.writes.insert((object_id, version));
                            accesses.count_own += 1;
                        }
                    } else {
                        accesses.writes.insert((object_id, version));
                        accesses.count_own += 1;
                    }
                } else if let &SuiObjectArg::SharedObject { object_id, initial_shared_version, mutable } = obj {
                    if mutable {
                        accesses.writes.insert((object_id, initial_shared_version));
                        accesses.count_shw += 1;
                    } else {
                        accesses.reads.insert((object_id, initial_shared_version));
                        accesses.count_shr += 1;
                    }
                }
            }
        }

        for cmd in &tx.commands {
            if let SuiCommand::MoveCall(call) = cmd {
                let object_id = call.package;
                let opt = SuiObjectDataOptions::new().with_owner();
                //let obj = sui.read_api().get_object_with_options(object_id, opt).await?;
                //let version = obj.data.as_ref().unwrap().version;
                //assert!(obj.data.unwrap().owner.unwrap().is_immutable());
                accesses.reads.insert((object_id, 0.into()));
                accesses.count_imm += 1;
            }
        }

        for obj_ref in effects.mutated.iter().map(|owned_ref| &owned_ref.reference)
            .chain(effects.unwrapped.iter().map(|owned_ref| &owned_ref.reference))
            .chain(effects.wrapped.iter())
            .chain(effects.unwrapped_then_deleted.iter())
            .chain(effects.deleted.iter()) {
            if accesses.writes.insert((obj_ref.object_id, obj_ref.version)) {
                accesses.count_oth += 1;
            }
        }
    }

    Ok(accesses)
}

/// 
fn list_scheduling(gas: &Vec<u64>, conflicts: &HashSet<(usize, usize)>) -> (u64, u64) {
    let total_gas = gas.iter().sum();
    let mut sequential_gas = 0;
    let mut executing = BinaryHeap::new();
    let mut executing_idxs = HashSet::new();
    let mut executed_idxs = HashSet::new();

    loop {
        'outer: for (i, tx_gas) in gas.iter().enumerate() {
            if executed_idxs.contains(&i) {
                continue;
            }
            for &j in &executing_idxs {
                if conflicts.contains(&(i, j)) || conflicts.contains(&(j, i)) {
                    continue 'outer;
                }
            }
            executing.push(Reverse((sequential_gas + tx_gas, i)));
            executing_idxs.insert(i);
            executed_idxs.insert(i);
        }

        if executing.is_empty() {
            break;
        }

        let (cur_gas, cur_idx) = executing.pop().unwrap().0;
        executing_idxs.remove(&cur_idx);
        sequential_gas = cur_gas;
    }

    (total_gas, sequential_gas)
}

/// 
fn print_progress(current: u64, total: u64, start_time: &Instant) {
    let speed = current as f64 / start_time.elapsed().as_secs_f64();
    let left = total - current;
    let eta = Duration::from_secs_f64(left as f64 / speed);
    let secs = eta.as_secs() % 60;
    let mins = eta.as_secs() / 60 % 60;
    let hours = eta.as_secs() / 3600;
    eprintln!("{:.2} tx/s, ETA: {}h {}m {}s", speed, hours, mins, secs);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_scheduling() {
        let gas = vec![2, 2, 2, 3];
        let mut conflicts = HashSet::new();
        conflicts.insert((0, 1));
        conflicts.insert((0, 2));
        conflicts.insert((1, 2));
        let (total, seq) = list_scheduling(&gas, &conflicts);
        assert_eq!(total, 9);
        assert_eq!(seq, 6);
    }
}
