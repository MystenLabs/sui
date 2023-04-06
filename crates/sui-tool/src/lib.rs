// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::StreamExt;
use futures::{
    future::{join_all, ready},
    FutureExt,
};
use itertools::Itertools;
use shared_crypto::intent::Intent;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, io};
use sui::client_commands::WalletContext;
use sui_config::{genesis::Genesis, NodeConfig, SUI_CLIENT_CONFIG};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClient};
use sui_keys::keystore::AccountKeystore;
use sui_network::default_mysten_network_config;
use sui_types::crypto::SignatureScheme;
use sui_types::multiaddr::Multiaddr;
use sui_types::object::ObjectFormatOptions;
use sui_types::SUI_CLOCK_OBJECT_ID;
use sui_types::{base_types::*, message_envelope::Message, messages::*, object::Owner};
use tokio::time::Instant;
use typed_store::traits::Map;

use anyhow::anyhow;

pub mod commands;
pub mod db_tool;

fn make_clients(
    genesis: PathBuf,
) -> Result<BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>> {
    let net_config = default_mysten_network_config();

    let genesis = Genesis::load(genesis)?;

    let mut authority_clients = BTreeMap::new();

    for validator in genesis.validator_set_for_tooling() {
        let metadata = validator.verified_metadata();
        let channel = net_config
            .connect_lazy(&metadata.net_address)
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel);
        let public_key_bytes = metadata.sui_pubkey_bytes();
        authority_clients.insert(public_key_bytes, (metadata.net_address.clone(), client));
    }

    Ok(authority_clients)
}

type ObjectVersionResponses = Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>, f64)>;
pub struct ObjectData {
    requested_id: ObjectID,
    responses: Vec<(AuthorityName, Multiaddr, ObjectVersionResponses)>,
}

trait OptionDebug<T> {
    fn opt_debug(&self, def_str: &str) -> String;
}
trait OptionDisplay<T> {
    fn opt_display(&self, def_str: &str) -> String;
}

impl<T> OptionDebug<T> for Option<T>
where
    T: std::fmt::Debug,
{
    fn opt_debug(&self, def_str: &str) -> String {
        match self {
            None => def_str.to_string(),
            Some(t) => format!("{:?}", t),
        }
    }
}

impl<T> OptionDisplay<T> for Option<T>
where
    T: std::fmt::Display,
{
    fn opt_display(&self, def_str: &str) -> String {
        match self {
            None => def_str.to_string(),
            Some(t) => format!("{}", t),
        }
    }
}

struct OwnerOutput(Owner);

// grep/awk-friendly output for Owner
impl std::fmt::Display for OwnerOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Owner::AddressOwner(address) => {
                write!(f, "address({})", address)
            }
            Owner::ObjectOwner(address) => {
                write!(f, "object({})", address)
            }
            Owner::Immutable => {
                write!(f, "immutable")
            }
            Owner::Shared { .. } => {
                write!(f, "shared")
            }
        }
    }
}

pub struct GroupedObjectOutput(pub ObjectData);

#[allow(clippy::format_in_format_args)]
impl std::fmt::Display for GroupedObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let responses = self
            .0
            .responses
            .iter()
            .flat_map(|(name, multiaddr, resp)| {
                resp.iter().map(|(seq_num, r, timespent)| {
                    (*name, multiaddr.clone(), seq_num, r, timespent)
                })
            })
            .sorted_by(|a, b| Ord::cmp(&b.2, &a.2))
            .group_by(|(_, _, seq_num, _r, _ts)| **seq_num);
        for (seq_num, group) in &responses {
            writeln!(f, "seq num: {}", seq_num.opt_debug("latest-seq-num"))?;
            let cur_version_resp = group.group_by(|(_, _, _, r, _)| match r {
                Ok(result) => {
                    let parent_tx_digest = result.object.previous_transaction;
                    let obj_digest = result.object.compute_object_reference().2;
                    let lock = result
                        .lock_for_debugging
                        .as_ref()
                        .map(|lock| *lock.digest());
                    let owner = result.object.owner;
                    Some((parent_tx_digest, obj_digest, lock, owner))
                }
                Err(_) => None,
            });
            for (result, group) in &cur_version_resp {
                match result {
                    Some((parent_tx_digest, obj_digest, lock, owner)) => {
                        let lock = lock.opt_debug("no-known-lock");
                        writeln!(f, "obj ref: {obj_digest}")?;
                        writeln!(f, "parent tx: {parent_tx_digest}")?;
                        writeln!(f, "owner: {owner}")?;
                        writeln!(f, "lock: {lock}")?;
                        for (i, (name, multiaddr, _, _, timespent)) in group.enumerate() {
                            writeln!(
                                f,
                                "        {:<4} {:<66} {:<56} (using {:.3} seconds)",
                                i,
                                name,
                                format!("{}", multiaddr),
                                timespent
                            )?;
                        }
                    }
                    None => {
                        writeln!(f, " error")?;
                        for (i, (name, multiaddr, _, resp, timespent)) in group.enumerate() {
                            writeln!(
                                f,
                                "        {:<4} {:<66} {:<56} (using {:.3} seconds) {:?}",
                                i,
                                name,
                                format!("{}", multiaddr),
                                timespent,
                                resp
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct ConciseObjectOutput(ObjectData);

impl ConciseObjectOutput {
    fn header() -> String {
        format!(
            "{:<66} {:<8} {:<66} {:<45} {}",
            "validator", "version", "digest", "parent_cert", "owner"
        )
    }
}

impl std::fmt::Display for ConciseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, _multi_addr, versions) in &self.0.responses {
            for (version, resp, _time_elapsed) in versions {
                write!(
                    f,
                    "{:<66} {:<8}",
                    format!("{:?}", name),
                    version.map(|s| s.value()).opt_debug("-")
                )?;
                match resp {
                    Err(_) => writeln!(
                        f,
                        "{:<66} {:<45} {:<51}",
                        "object-fetch-failed", "no-cert-available", "no-owner-available"
                    )?,
                    Ok(resp) => {
                        let obj_digest = resp.object.compute_object_reference().2;
                        let parent = resp.object.previous_transaction;
                        let owner = resp.object.owner;
                        write!(f, " {:<66} {:<45} {:<51}", obj_digest, parent, owner)?;
                    }
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

struct VerboseObjectOutput(ObjectData);

impl std::fmt::Display for VerboseObjectOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Object: {}", self.0.requested_id)?;

        for (name, multiaddr, versions) in &self.0.responses {
            writeln!(f, "validator: {:?}, addr: {:?}", name, multiaddr)?;

            for (version, resp, timespent) in versions {
                writeln!(
                    f,
                    "-- version: {} (using {:.3} seconds)",
                    version.opt_debug("<version not available>"),
                    timespent,
                )?;

                match resp {
                    Err(e) => writeln!(f, "Error fetching object: {}", e)?,
                    Ok(resp) => {
                        writeln!(
                            f,
                            "  -- object digest: {}",
                            resp.object.compute_object_reference().2
                        )?;
                        if resp.object.is_package() {
                            writeln!(f, "  -- object: <Move Package>")?;
                        } else if let Some(layout) = &resp.layout {
                            writeln!(
                                f,
                                "  -- object: Move Object: {}",
                                resp.object
                                    .data
                                    .try_as_move()
                                    .unwrap()
                                    .to_move_struct(layout)
                                    .unwrap()
                            )?;
                        }
                        writeln!(f, "  -- owner: {}", resp.object.owner)?;
                        writeln!(
                            f,
                            "  -- locked by: {}",
                            resp.lock_for_debugging.opt_debug("<not locked>")
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub async fn get_object(
    obj_id: ObjectID,
    version: Option<u64>,
    validator: Option<AuthorityName>,
    genesis: PathBuf,
    history: bool,
) -> Result<ObjectData> {
    let clients = make_clients(genesis)?;

    let responses = join_all(
        clients
            .iter()
            .filter(|(name, _)| {
                if let Some(v) = validator {
                    v == **name
                } else {
                    true
                }
            })
            .map(|(name, (address, client))| async {
                let object_versions = get_object_impl(client, obj_id, version, history).await;
                (*name, address.clone(), object_versions)
            }),
    )
    .await;

    Ok(ObjectData {
        requested_id: obj_id,
        responses,
    })
}

struct ReadTxFromClients {
    clients: BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>,
}

impl ReadTxFromClients {
    fn new(clients: BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>) -> Self {
        Self { clients }
    }
}

struct ReadTxFromDb {
    db: AuthorityPerpetualTables,
}

impl ReadTxFromDb {
    fn new(db_path: PathBuf) -> Self {
        Self {
            db: AuthorityPerpetualTables::open(&db_path, None),
        }
    }
}

#[async_trait]
trait ReadTx {
    async fn fetch_txn(
        &self,
        tx_digest: TransactionDigest,
        fx_digest: Option<TransactionEffectsDigest>,
    ) -> Result<(SenderSignedData, TransactionEffects)>;
}

#[async_trait]
impl ReadTx for ReadTxFromClients {
    async fn fetch_txn(
        &self,
        tx_digest: TransactionDigest,
        fx_digest: Option<TransactionEffectsDigest>,
    ) -> Result<(SenderSignedData, TransactionEffects)> {
        // get first successful response from futures
        async fn poll_first_response(
            tx_digest: TransactionDigest,
            fx_digest: Option<TransactionEffectsDigest>,
            clients: &BTreeMap<AuthorityName, (Multiaddr, NetworkAuthorityClient)>,
        ) -> (SenderSignedData, TransactionEffects) {
            let mut futures: FuturesUnordered<_> = clients
                .iter()
                .map(|(name, (address, client))| async {
                    let result = client
                        .handle_transaction_info_request(TransactionInfoRequest {
                            transaction_digest: tx_digest,
                        })
                        .await;
                    (*name, address.clone(), result)
                })
                .collect();

            while let Some((name, address, result)) = futures.next().await {
                match result {
                    Ok(resp) => {
                        if resp.transaction.digest() != tx_digest {
                            eprintln!("transaction digest mismatch");
                            continue;
                        }
                        match resp.status {
                            TransactionStatus::Executed(_, effects, _) => {
                                if let Some(fx_digest) = fx_digest {
                                    if *effects.digest() != fx_digest {
                                        eprintln!("effects digest mismatch");
                                        continue;
                                    }
                                } else {
                                    return (resp.transaction, effects.into_data());
                                }
                            }
                            _ => {
                                eprintln!("transaction not executed");
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Error fetching transaction from validator {:?} at {:?}: {}",
                            name, address, e
                        );
                    }
                }
            }
            panic!("No validator responded to transaction request");
        }

        let ret = poll_first_response(tx_digest, fx_digest, &self.clients).await;
        Ok(ret)
    }
}

#[async_trait]
impl ReadTx for ReadTxFromDb {
    async fn fetch_txn(
        &self,
        tx_digest: TransactionDigest,
        fx_digest: Option<TransactionEffectsDigest>,
    ) -> Result<(SenderSignedData, TransactionEffects)> {
        let tx: Option<VerifiedTransaction> = self
            .db
            .transactions
            .get(&tx_digest)
            .unwrap()
            .map(|v| v.into())
            .clone();
        let tx = tx
            .unwrap_or_else(|| {
                panic!("transaction not found in db: {:?}", tx_digest);
            })
            .data()
            .clone();

        assert_eq!(tx_digest, tx.digest());
        let executed_digest = self.db.executed_effects.get(&tx_digest).unwrap().unwrap();
        if let Some(fx_digest) = fx_digest {
            assert_eq!(fx_digest, executed_digest);
        }

        let fx = self.db.effects.get(&executed_digest).unwrap().unwrap();

        Ok((tx, fx))
    }
}

pub type CausalHistory = (
    Vec<TransactionDigest>,
    BTreeMap<TransactionDigest, (SenderSignedData, TransactionEffects)>,
);

pub async fn replay_transactions(
    transactions: CausalHistory,
    mut address_map: BTreeMap<SuiAddress, SuiAddress>,
    working_dir: PathBuf,
) {
    let config_path = working_dir.join(SUI_CLIENT_CONFIG);
    let mut context = WalletContext::new(&config_path, None).await.unwrap();

    let (roots, txns) = transactions;
    let mut forward_deps: BTreeMap<TransactionDigest, Vec<TransactionDigest>> = BTreeMap::new();
    let mut backward_deps: BTreeMap<TransactionDigest, HashSet<TransactionDigest>> =
        BTreeMap::new();

    for (tx_digest, (_, fx)) in txns.iter() {
        let deps = fx.dependencies();
        forward_deps.insert(*tx_digest, deps.to_vec());
        for dep in deps {
            backward_deps
                .entry(*dep)
                .or_insert_with(HashSet::new)
                .insert(*tx_digest);
        }
    }

    let txns: BTreeMap<TransactionDigest, VerifiedTransaction> = txns
        .into_iter()
        .map(|(tx_digest, (tx, _))| {
            let mut tx_data = tx.transaction_data().clone();
            let sender = tx_data.sender_mut();

            let new_sender = if let Some(new_sender) = address_map.get(sender) {
                *new_sender
            } else {
                let (new_sender, _, _) = context
                    .config
                    .keystore
                    .generate_and_add_new_key(SignatureScheme::ED25519, None)
                    .unwrap();
                address_map.insert(*sender, new_sender);
                new_sender
            };

            *sender = new_sender;
            let sender = *sender;

            // re-sign the tx
            let signature = context
                .config
                .keystore
                .sign_secure(&sender, &tx_data, Intent::default())
                .unwrap();

            (
                tx_digest,
                Transaction::from_data(tx_data.clone(), Intent::default(), vec![signature])
                    .verify()
                    .unwrap_or_else(|_| panic!("tx_data: {:#?}", tx_data)),
            )
        })
        .collect();

    let mut root_futures = FuturesUnordered::new();
    let mut finished = FuturesUnordered::new();
    root_futures.extend(roots.into_iter().map(|root| ready(root).boxed()));

    let context = Arc::new(context);

    loop {
        tokio::select! {
            Some(root) = root_futures.next() => {
                let context = context.clone();
                let txn = txns.get(&root).unwrap().clone();
                finished.push(tokio::task::spawn(async move {
                    context.execute_transaction_block(txn).await.unwrap();
                    root
                }));
            }

            Some(Ok(tx_digest)) = finished.next() => {
                if let Some(deps) = forward_deps.get(&tx_digest) {
                    for dep in deps {
                        if let Some(deps) = backward_deps.get_mut(dep) {
                            deps.remove(&tx_digest);
                            if deps.is_empty() {
                                root_futures.push(ready(*dep).boxed());
                            }
                        }
                    }
                }
            }

            else => break,
        }
    }

    // rea
}

pub async fn fetch_causal_history(
    tx_digest: TransactionDigest,
    fx_digest: Option<TransactionEffectsDigest>,
    genesis: PathBuf,
    db_path: Option<PathBuf>,
) -> Result<CausalHistory> {
    let fetcher: Box<dyn ReadTx> = if let Some(db_path) = db_path {
        Box::new(ReadTxFromDb::new(db_path))
    } else {
        let clients = make_clients(genesis)?;
        Box::new(ReadTxFromClients::new(clients))
    };

    let mut results = BTreeMap::new();
    let mut roots = Vec::new();
    let mut processed = HashSet::new();

    let mut queue = Vec::new();
    let (tx, fx) = fetcher.fetch_txn(tx_digest, fx_digest).await?;

    results.insert(tx_digest, (tx.clone(), fx.clone()));
    queue.push(fx.dependencies().to_vec());

    while let Some(deps) = queue.pop() {
        let futures: FuturesOrdered<_> = deps
            .into_iter()
            .filter(|dep| processed.insert(*dep))
            .map(|dep| fetcher.fetch_txn(dep, None))
            .collect();

        // collect all results from `futures` into a vector
        let dep_results = futures.collect::<Vec<_>>().await;

        for res in dep_results {
            let (tx, fx) = res?;
            let tx_digest = tx.digest();

            let mutated = fx.mutated();
            if mutated.len() == 1 && mutated[0].0 .0 == SUI_CLOCK_OBJECT_ID {
                continue;
            }

            assert!(results
                .insert(tx_digest, (tx.clone(), fx.clone()))
                .is_none());

            if fx.dependencies().is_empty() {
                roots.push(tx_digest);
            } else {
                queue.push(fx.dependencies().to_vec());
            }
        }
    }

    Ok((roots, results))
}

pub async fn get_transaction_block(
    tx_digest: TransactionDigest,
    genesis: PathBuf,
) -> Result<String> {
    let clients = make_clients(genesis)?;
    let timer = Instant::now();
    let responses = join_all(clients.iter().map(|(name, (address, client))| async {
        let result = client
            .handle_transaction_info_request(TransactionInfoRequest {
                transaction_digest: tx_digest,
            })
            .await;
        (
            *name,
            address.clone(),
            result,
            timer.elapsed().as_secs_f64(),
        )
    }))
    .await;

    let responses = responses
        .iter()
        .map(|r| {
            let key =
                r.2.as_ref()
                    .map(|ok_result| match &ok_result.status {
                        TransactionStatus::Signed(_) => None,
                        TransactionStatus::Executed(_, effects, _) => Some(effects.digest()),
                    })
                    .ok();
            (key, r)
        })
        .sorted_by(|(k1, _), (k2, _)| Ord::cmp(k1, k2))
        .group_by(|(_, r)| {
            r.2.as_ref().map(|ok_result| match &ok_result.status {
                TransactionStatus::Signed(_) => None,
                TransactionStatus::Executed(_, effects, _) => {
                    Some((effects.data(), effects.digest()))
                }
            })
        });
    let mut s = String::new();
    for (i, (key, group)) in responses.into_iter().enumerate() {
        match key {
            Ok(Some((effects, effects_digest))) => {
                writeln!(
                    &mut s,
                    "#{:<2} tx_digest: {:<68?} effects_digest: {:?}",
                    i, tx_digest, effects_digest,
                )?;
                writeln!(&mut s, "{:#?}", effects)?;
            }
            Ok(None) => {
                writeln!(
                    &mut s,
                    "#{:<2} tx_digest: {:<68?} Signed but not executed",
                    i, tx_digest
                )?;
            }
            other => {
                writeln!(&mut s, "#{:<2} {:#?}", i, other)?;
            }
        }
        for (j, (_, res)) in group.enumerate() {
            writeln!(
                &mut s,
                "        {:<4} {:<66} {:<56} (using {:.3} seconds)",
                j,
                res.0,
                format!("{}", res.1),
                res.3
            )?;
        }
        writeln!(&mut s)?;
    }
    Ok(s)
}

async fn get_object_impl(
    client: &NetworkAuthorityClient,
    id: ObjectID,
    start_version: Option<u64>,
    full_history: bool,
) -> Vec<(Option<SequenceNumber>, Result<ObjectInfoResponse>, f64)> {
    let mut ret = Vec::new();
    let mut version = start_version;

    loop {
        let start = Instant::now();
        let resp = client
            .handle_object_info_request(ObjectInfoRequest {
                object_id: id,
                object_format_options: Some(ObjectFormatOptions::default()),
                request_kind: match version {
                    None => ObjectInfoRequestKind::LatestObjectInfo,
                    Some(v) => {
                        ObjectInfoRequestKind::PastObjectInfoDebug(SequenceNumber::from_u64(v))
                    }
                },
            })
            .await
            .map_err(anyhow::Error::from);
        let elapsed = start.elapsed().as_secs_f64();

        let resp_version = resp.as_ref().ok().map(|r| r.object.version().value());
        ret.push((resp_version.map(SequenceNumber::from), resp, elapsed));

        version = match (version, resp_version) {
            (Some(v), _) | (None, Some(v)) => {
                if v == 1 || !full_history {
                    break;
                } else {
                    // TODO: With lamport versioning, this is very inefficient.
                    Some(v - 1)
                }
            }
            _ => break,
        };
    }

    ret
}

pub(crate) fn make_anemo_config() -> anemo_cli::Config {
    use narwhal_types::*;
    use sui_network::discovery::*;
    use sui_network::state_sync::*;

    // TODO: implement `ServiceInfo` generation in anemo-build and use here.
    anemo_cli::Config::new()
        // Narwhal primary-to-primary
        .add_service(
            "PrimaryToPrimary",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "SendCertificate",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        send_certificate,
                        SendCertificateRequest
                    ),
                )
                .add_method(
                    "RequestVote",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        request_vote,
                        RequestVoteRequest
                    ),
                )
                .add_method(
                    "GetPayloadAvailability",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        get_payload_availability,
                        PayloadAvailabilityRequest
                    ),
                )
                .add_method(
                    "GetCertificates",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        get_certificates,
                        GetCertificatesRequest
                    ),
                )
                .add_method(
                    "FetchCertificates",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        fetch_certificates,
                        FetchCertificatesRequest
                    ),
                ),
        )
        // Narwhal worker-to-worker
        .add_service(
            "WorkerToWorker",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "ReportBatch",
                    anemo_cli::ron_method!(WorkerToWorkerClient, report_batch, WorkerBatchMessage),
                )
                .add_method(
                    "RequestBatch",
                    anemo_cli::ron_method!(
                        WorkerToWorkerClient,
                        request_batch,
                        RequestBatchRequest
                    ),
                ),
        )
        // Sui discovery
        .add_service(
            "Discovery",
            anemo_cli::ServiceInfo::new().add_method(
                "GetKnownPeers",
                anemo_cli::ron_method!(DiscoveryClient, get_known_peers, ()),
            ),
        )
        // Sui state sync
        .add_service(
            "StateSync",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "PushCheckpointSummary",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        push_checkpoint_summary,
                        sui_types::messages_checkpoint::CertifiedCheckpointSummary
                    ),
                )
                .add_method(
                    "GetCheckpointSummary",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_checkpoint_summary,
                        GetCheckpointSummaryRequest
                    ),
                )
                .add_method(
                    "GetCheckpointContents",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_checkpoint_contents,
                        sui_types::messages_checkpoint::CheckpointContentsDigest
                    ),
                ),
        )
}

fn copy_dir_all(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    skip: Vec<PathBuf>,
) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if skip.contains(&entry.path()) {
            continue;
        }
        if ty.is_dir() {
            copy_dir_all(
                entry.path(),
                dst.as_ref().join(entry.file_name()),
                skip.clone(),
            )?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub async fn restore_from_db_checkpoint(
    config: &NodeConfig,
    db_checkpoint_path: &Path,
) -> Result<(), anyhow::Error> {
    copy_dir_all(db_checkpoint_path, config.db_path(), vec![])?;
    Ok(())
}
