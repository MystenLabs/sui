// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `LedgerService/GetObject`, `BatchGetObjects` and `ListObjectVersions` on a fullnode and on
//! kv-rpc indexing the same chain.

use std::collections::BTreeSet;
use std::time::Duration;

use itertools::Itertools;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use prometheus::Registry;
use prost::Message;
use prost::bytes::Bytes;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_kv_rpc::KvRpcConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::ErrorInfo;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsResponse;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::Ordering;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::QueryOptions;
use sui_rpc::proto::sui::rpc::v2::get_object_result;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::object_removal;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use tempfile::TempDir;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tonic::transport::Channel;

/// The `Object` fields both servers render from the stored object.
const OBJECT_FIELDS: &[&str] = &[
    "object_id",
    "version",
    "digest",
    "owner",
    "object_type",
    "previous_transaction",
    "contents",
];

/// Items per `ListObjectVersions` page when paging, small enough that every history spans pages.
const PAGE_SIZE: u32 = 2;

/// Requests per `BatchGetObjects` call.
const BATCH_SIZE: usize = 100;

/// One change the test made to an object.
#[derive(Clone, Copy, Debug)]
struct Change {
    checkpoint: u64,
    id: ObjectID,
    version: u64,
    /// For a deletion or wrap, its kind and the transaction that made it.
    removal: Option<(object_removal::Kind, TransactionDigest)>,
}

/// Every change the test made, in the order it made them.
#[derive(Default)]
struct Changes(Vec<Change>);

impl Changes {
    fn live(&mut self, checkpoint: u64, id: ObjectID, version: SequenceNumber) {
        self.0.push(Change {
            checkpoint,
            id,
            version: version.value(),
            removal: None,
        });
    }

    fn removed(
        &mut self,
        checkpoint: u64,
        id: ObjectID,
        version: SequenceNumber,
        kind: object_removal::Kind,
        transaction: TransactionDigest,
    ) {
        self.0.push(Change {
            checkpoint,
            id,
            version: version.value(),
            removal: Some((kind, transaction)),
        });
    }

    /// The objects the test changed, in the order of their first change.
    fn ids(&self) -> Vec<ObjectID> {
        let mut seen = BTreeSet::new();
        self.0
            .iter()
            .map(|change| change.id)
            .filter(|id| seen.insert(*id))
            .collect()
    }

    /// `id`'s changes, in version order, which is also checkpoint order.
    fn history(&self, id: ObjectID) -> Vec<Change> {
        let history: Vec<Change> = self.0.iter().filter(|c| c.id == id).copied().collect();
        assert!(
            history
                .windows(2)
                .all(|pair| pair[0].version < pair[1].version
                    && pair[0].checkpoint <= pair[1].checkpoint),
            "{history:?}"
        );
        history
    }
}

/// What a server answered to one object read.
#[derive(Clone, Debug, PartialEq)]
enum Answer {
    Found(Box<Object>),
    Refused(Refusal),
}

/// A failed read or listing: the status code and message, and the `ErrorInfo` details.
#[derive(Clone, Debug, PartialEq)]
struct Refusal {
    code: tonic::Code,
    message: String,
    error_infos: Vec<ErrorInfo>,
}

impl Refusal {
    fn from_status(status: &sui_rpc::proto::google::rpc::Status) -> Self {
        Self {
            code: tonic::Code::from(status.code),
            message: status.message.clone(),
            error_infos: status
                .details
                .iter()
                .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
                .collect(),
        }
    }

    fn from_tonic(status: &tonic::Status) -> Self {
        let details = sui_rpc::proto::google::rpc::Status::decode(status.details())
            .expect("error details should decode as google.rpc.Status");
        Self {
            code: status.code(),
            message: status.message().to_owned(),
            ..Self::from_status(&details)
        }
    }

    /// The `OBJECT_REMOVED` detail, as `(removedVersion, removalKind, transaction)`.
    fn removal(&self) -> Option<(u64, String, Option<String>)> {
        let info = self
            .error_infos
            .iter()
            .find(|info| info.reason == ErrorReason::ObjectRemoved.as_str_name())?;
        assert_eq!(info.domain, ErrorReason::DOMAIN, "{info:?}");
        Some((
            info.metadata["removedVersion"].parse().unwrap(),
            info.metadata["removalKind"].clone(),
            info.metadata.get("transaction").cloned(),
        ))
    }
}

/// The answer the chain implies for a read that selects `selected`, kept only if it was made by
/// the end of `at_checkpoint`: its version if live, else `NotFound` naming its removal, and plain
/// `NotFound` if nothing is selected.
#[derive(Debug)]
enum Expected {
    Version(u64),
    Removed(Change),
    NotFound,
}

fn expected(selected: Option<&Change>, at_checkpoint: Option<u64>) -> Expected {
    match selected.filter(|change| at_checkpoint.is_none_or(|bound| change.checkpoint <= bound)) {
        Some(change) if change.removal.is_none() => Expected::Version(change.version),
        Some(change) => Expected::Removed(*change),
        None => Expected::NotFound,
    }
}

fn assert_expected(answer: &Answer, id: ObjectID, expected: &Expected, context: &str) {
    match (answer, expected) {
        (Answer::Found(object), Expected::Version(version)) => {
            assert_eq!(
                object.object_id.as_deref(),
                Some(id.to_canonical_string(true).as_str()),
                "{context}"
            );
            assert_eq!(object.version, Some(*version), "{context}");
        }
        (Answer::Refused(refusal), Expected::NotFound) => {
            assert_eq!(
                refusal.code,
                tonic::Code::NotFound,
                "{context}: {refusal:?}"
            );
            assert_eq!(refusal.removal(), None, "{context}: {refusal:?}");
        }
        (Answer::Refused(refusal), Expected::Removed(change)) => {
            assert_eq!(
                refusal.code,
                tonic::Code::NotFound,
                "{context}: {refusal:?}"
            );
            let (kind, transaction) = change.removal.unwrap();
            assert_eq!(
                refusal.removal(),
                Some((
                    change.version,
                    kind.as_str_name().to_owned(),
                    Some(transaction.to_string())
                )),
                "{context}: {refusal:?}"
            );
        }
        (answer, expected) => panic!("{context}: expected {expected:?}, got {answer:?}"),
    }
}

async fn get_object(
    client: &mut LedgerServiceClient<Channel>,
    mut req: GetObjectRequest,
) -> Answer {
    req.read_mask = Some(FieldMask::from_paths(OBJECT_FIELDS.iter().copied()));
    match client.get_object(req).await {
        Ok(response) => Answer::Found(Box::new(
            response
                .into_inner()
                .object
                .expect("object should be present"),
        )),
        Err(status) => Answer::Refused(Refusal::from_tonic(&status)),
    }
}

async fn batch_get_objects(
    client: &mut LedgerServiceClient<Channel>,
    requests: Vec<GetObjectRequest>,
) -> Vec<Answer> {
    let len = requests.len();
    let mut batch = BatchGetObjectsRequest::default();
    batch.requests = requests;
    batch.read_mask = Some(FieldMask::from_paths(OBJECT_FIELDS.iter().copied()));
    let response = client
        .batch_get_objects(batch)
        .await
        .unwrap_or_else(|status| panic!("BatchGetObjects: {status:?}"))
        .into_inner();
    assert_eq!(response.objects.len(), len, "one result per request");
    response
        .objects
        .into_iter()
        .map(|item| match item.result.expect("result should be set") {
            get_object_result::Result::Object(object) => Answer::Found(Box::new(object)),
            get_object_result::Result::Error(status) => {
                Answer::Refused(Refusal::from_status(&status))
            }
            other => panic!("unexpected batch result: {other:?}"),
        })
        .collect()
}

fn object_request(
    id: ObjectID,
    version: Option<u64>,
    max_version: Option<u64>,
    at_checkpoint: Option<u64>,
) -> GetObjectRequest {
    let mut req = GetObjectRequest::default();
    req.object_id = Some(id.to_canonical_string(true));
    req.version = version;
    req.max_version = max_version;
    req.at_checkpoint = at_checkpoint;
    req
}

fn describe(req: &GetObjectRequest) -> String {
    format!(
        "{} version {:?} max_version {:?} at_checkpoint {:?}",
        req.object_id.as_deref().unwrap_or_default(),
        req.version,
        req.max_version,
        req.at_checkpoint,
    )
}

/// Every frame of one `ListObjectVersions` stream, or the error it failed with.
type Listing = Result<Vec<ListObjectVersionsResponse>, Refusal>;

async fn list_object_versions(
    client: &mut LedgerServiceClient<Channel>,
    req: ListObjectVersionsRequest,
) -> Listing {
    let refusal = |status: tonic::Status| Refusal::from_tonic(&status);
    let mut stream = client
        .list_object_versions(req)
        .await
        .map_err(refusal)?
        .into_inner();
    let mut frames = vec![];
    while let Some(frame) = stream.message().await.map_err(refusal)? {
        frames.push(frame);
    }
    Ok(frames)
}

/// A listing request; `with_transactions` adds `transaction` to the default removal mask
/// (`version,kind`).
fn list_request(
    id: ObjectID,
    ordering: Ordering,
    limit: Option<u32>,
    cursor: Option<Bytes>,
    at_checkpoint: Option<u64>,
    with_transactions: bool,
) -> ListObjectVersionsRequest {
    let mut options = QueryOptions::default();
    options.limit = limit;
    options.ordering = Some(ordering as i32);
    // A page resumes past the previous page's last item: above it ascending, below it
    // descending.
    match ordering {
        Ordering::Descending => options.before = cursor,
        _ => options.after = cursor,
    }
    let mut req = ListObjectVersionsRequest::default();
    req.object_id = Some(id.to_canonical_string(true));
    req.read_mask = Some(FieldMask::from_paths(OBJECT_FIELDS.iter().copied()));
    if with_transactions {
        req.removal_read_mask = Some(FieldMask::from_paths(["version", "kind", "transaction"]));
    }
    req.options = Some(options);
    req.at_checkpoint = at_checkpoint;
    req
}

/// A listed item: its version, and for a removal, its kind and transaction.
type ListedItem = (u64, Option<(object_removal::Kind, Option<String>)>);

fn listed_items(listing: &Listing, context: &str) -> Vec<ListedItem> {
    let frames = listing
        .as_ref()
        .unwrap_or_else(|refusal| panic!("{context}: listing failed: {refusal:?}"));
    frames
        .iter()
        .filter_map(|frame| match (&frame.object, &frame.removal) {
            (Some(object), None) => Some((object.version.expect("version"), None)),
            (None, Some(removal)) => Some((
                removal.version.expect("removal version"),
                Some((removal.kind(), removal.transaction.clone())),
            )),
            (None, None) => None,
            (Some(_), Some(_)) => panic!("{context}: a frame has both an object and a removal"),
        })
        .collect()
}

fn end_reason(listing: &Listing, context: &str) -> QueryEndReason {
    let frames = listing
        .as_ref()
        .unwrap_or_else(|refusal| panic!("{context}: listing failed: {refusal:?}"));
    let end = frames
        .last()
        .and_then(|frame| frame.end)
        .unwrap_or_else(|| panic!("{context}: the stream ends without QueryEnd"));
    QueryEndReason::try_from(end.reason.expect("reason should be set")).expect("known reason")
}

fn last_cursor(listing: &Listing) -> Option<Bytes> {
    listing
        .as_ref()
        .ok()?
        .iter()
        .rev()
        .find_map(|frame| frame.cursor.clone())
}

/// The items listing `history` gives, with each removal's transaction if `with_transactions`.
fn model_items(history: &[Change], with_transactions: bool) -> Vec<ListedItem> {
    history
        .iter()
        .map(|change| {
            (
                change.version,
                change.removal.map(|(kind, transaction)| {
                    (kind, with_transactions.then(|| transaction.to_string()))
                }),
            )
        })
        .collect()
}

/// The reference `effects` wrote for `id`.
fn output_ref(effects: &TransactionEffects, id: ObjectID) -> ObjectRef {
    effects
        .all_changed_objects()
        .into_iter()
        .find_map(|(object_ref, _, _)| (object_ref.0 == id).then_some(object_ref))
        .unwrap_or_else(|| panic!("no output for {id} in {effects:?}"))
}

/// The one object `effects` created.
fn created_object(effects: &TransactionEffects) -> ObjectRef {
    let created = effects.created();
    assert_eq!(created.len(), 1, "{created:?}");
    created[0].0
}

fn owned_arg(object_ref: ObjectRef) -> CallArg {
    CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref))
}

fn pure_arg<T: serde::Serialize>(value: &T) -> CallArg {
    CallArg::Pure(bcs::to_bytes(value).unwrap())
}

/// Execute `data` through the fullnode, returning its effects and checkpoint.
async fn execute(cluster: &TestCluster, data: TransactionData) -> (TransactionEffects, u64) {
    let executed = cluster
        .execute_transaction(cluster.sign_transaction(&data).await)
        .await;
    let checkpoint = executed
        .checkpoint
        .expect("the fullnode waits for the transaction's checkpoint");
    (executed.effects, checkpoint)
}

async fn transaction_checkpoint(
    client: &mut LedgerServiceClient<Channel>,
    digest: TransactionDigest,
) -> u64 {
    let mut req = GetTransactionRequest::default();
    req.digest = Some(digest.to_string());
    req.read_mask = Some(FieldMask::from_paths(["checkpoint"]));
    client
        .get_transaction(req)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .and_then(|transaction| transaction.checkpoint)
        .expect("checkpoint should be set")
}

/// Record in `mismatches` a difference between the fullnode's and kv-rpc's answers to `what`.
fn compare<T: PartialEq + std::fmt::Debug>(
    mismatches: &mut Vec<String>,
    what: String,
    fullnode: &T,
    kv_rpc: &T,
) {
    if fullnode != kv_rpc {
        mismatches.push(format!(
            "{what}\n  fullnode: {fullnode:?}\n  kv-rpc:   {kv_rpc:?}"
        ));
    }
}

/// A fullnode and kv-rpc give the same answers to object reads over the same chain.
///
/// The chain drives objects through every kind of change: one through create, mutate, wrap,
/// unwrap and delete (its wrapper is created and deleted on the way); a shared counter mutated
/// twice in one checkpoint, then deleted; and a dynamic field deleted and re-created under the
/// same ID. For each object, both servers answer latest, exact-`version`, `max_version` and
/// `at_checkpoint` reads, alone and combined, at and around every version and checkpoint it
/// changed at, through `GetObject` and `BatchGetObjects`, and list its versions in both orders,
/// whole, bounded by those checkpoints, and in pages that each resume from a cursor the other
/// server minted. kv-rpc's answers are also checked against the chain.
#[tokio::test]
async fn test_object_reads_match_between_fullnode_and_kv_rpc() {
    let ingestion_dir = TempDir::new().unwrap();
    let onchain = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_data_ingestion_dir(ingestion_dir.path().to_path_buf())
        .with_epoch_duration_ms(300_000)
        .disable_fullnode_pruning()
        .build()
        .await;
    onchain.wait_for_rpc_index_ready().await;
    let mut fullnode = LedgerServiceClient::connect(onchain.rpc_url().to_owned())
        .await
        .unwrap();

    let offchain = OffchainCluster::new(
        ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(ingestion_dir.path().to_path_buf()),
                ..Default::default()
            },
            ..Default::default()
        },
        OffchainClusterConfig {
            kv_rpc_config: KvRpcConfig {
                enable_object_at_checkpoint: Some(true),
                ..Default::default()
            },
            ..Default::default()
        },
        &Registry::new(),
    )
    .await
    .unwrap();
    let mut kv_rpc = LedgerServiceClient::connect(offchain.kv_rpc_url().to_string())
        .await
        .unwrap();

    let accounts = onchain
        .wallet
        .get_all_accounts_and_gas_objects()
        .await
        .unwrap();
    let (sender, coins) = &accounts[0];
    let sender: SuiAddress = *sender;
    assert!(coins.len() >= 3, "{coins:?}");
    let rgp = onchain.get_reference_gas_price().await;
    let mut gas = coins[0];
    let builder = |gas: ObjectRef| TestTransactionBuilder::new(sender, gas, rgp);
    let mut changes = Changes::default();

    let (fx, _) = execute(
        &onchain,
        builder(gas).publish_examples("basics").await.build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    let package = fx
        .created()
        .into_iter()
        .find_map(|(object_ref, owner)| (owner == Owner::Immutable).then_some(object_ref.0))
        .expect("publish creates a package");

    // An owned object through create, mutate, wrap, unwrap and delete, each in a checkpoint of
    // its own.
    let (fx, checkpoint) = execute(
        &onchain,
        builder(gas)
            .move_call(
                package,
                "object_basics",
                "create",
                vec![pure_arg(&7u64), pure_arg(&sender)],
            )
            .build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    let mut object = created_object(&fx);
    changes.live(checkpoint, object.0, object.1);

    let (fx, checkpoint) = execute(
        &onchain,
        builder(gas)
            .move_call(
                package,
                "object_basics",
                "set_value",
                vec![owned_arg(object), pure_arg(&8u64)],
            )
            .build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    object = output_ref(&fx, object.0);
    changes.live(checkpoint, object.0, object.1);

    let (fx, checkpoint) = execute(
        &onchain,
        builder(gas)
            .move_call(package, "object_basics", "wrap", vec![owned_arg(object)])
            .build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    let wrapper = created_object(&fx);
    assert_eq!(fx.wrapped().len(), 1, "{fx:?}");
    changes.removed(
        checkpoint,
        object.0,
        fx.lamport_version(),
        object_removal::Kind::Wrapped,
        *fx.transaction_digest(),
    );
    changes.live(checkpoint, wrapper.0, wrapper.1);

    let (fx, checkpoint) = execute(
        &onchain,
        builder(gas)
            .move_call(package, "object_basics", "unwrap", vec![owned_arg(wrapper)])
            .build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    object = output_ref(&fx, object.0);
    changes.live(checkpoint, object.0, object.1);
    changes.removed(
        checkpoint,
        wrapper.0,
        fx.lamport_version(),
        object_removal::Kind::Deleted,
        *fx.transaction_digest(),
    );

    let (fx, checkpoint) = execute(
        &onchain,
        builder(gas)
            .move_call(package, "object_basics", "delete", vec![owned_arg(object)])
            .build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    changes.removed(
        checkpoint,
        object.0,
        fx.lamport_version(),
        object_removal::Kind::Deleted,
        *fx.transaction_digest(),
    );

    // A shared counter, incremented twice in one checkpoint by a soft bundle (its transactions
    // are sequenced in one consensus commit), then deleted.
    let (fx, checkpoint) =
        execute(&onchain, builder(gas).call_counter_create(package).build()).await;
    gas = output_ref(&fx, gas.0);
    let (counter, initial_shared_version) = fx
        .created()
        .into_iter()
        .find_map(|(object_ref, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((object_ref, initial_shared_version)),
            _ => None,
        })
        .expect("counter::create shares a counter");
    changes.live(checkpoint, counter.0, counter.1);

    let increments: Vec<TransactionData> = [coins[1], coins[2]]
        .into_iter()
        .map(|coin| {
            builder(coin)
                .call_counter_increment(package, counter.0, initial_shared_version)
                .build()
        })
        .collect();
    let bundle = onchain
        .sign_and_execute_txns_in_soft_bundle(&increments)
        .await
        .expect("soft bundle should execute");
    let mut increment_versions = vec![];
    for (digest, fx) in &bundle {
        assert!(fx.status().is_ok(), "{fx:?}");
        let checkpoint = transaction_checkpoint(&mut fullnode, *digest).await;
        increment_versions.push((output_ref(fx, counter.0).1, checkpoint));
    }
    increment_versions.sort();
    assert_eq!(
        increment_versions[0].1, increment_versions[1].1,
        "both increments land in one checkpoint: {increment_versions:?}"
    );
    for (version, checkpoint) in increment_versions {
        changes.live(checkpoint, counter.0, version);
    }

    let (fx, checkpoint) = execute(
        &onchain,
        builder(gas)
            .call_counter_delete(package, counter.0, initial_shared_version)
            .build(),
    )
    .await;
    gas = output_ref(&fx, gas.0);
    changes.removed(
        checkpoint,
        counter.0,
        fx.lamport_version(),
        object_removal::Kind::Deleted,
        *fx.transaction_digest(),
    );

    // A `Table<u64, u64>` whose key 1 is added, removed and added again, each in a checkpoint of
    // its own. The field keeps its ID, so its history continues after a deletion.
    let key_and_value_types = || vec![TypeTag::U64, TypeTag::U64];
    let mut new_table = builder(gas);
    let ptb = new_table.ptb_builder_mut();
    let table = ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("table").unwrap(),
        Identifier::new("new").unwrap(),
        key_and_value_types(),
        vec![],
    );
    ptb.transfer_arg(sender, table);
    let (fx, _) = execute(&onchain, new_table.build()).await;
    gas = output_ref(&fx, gas.0);
    let mut table = created_object(&fx);
    let mut field = None;
    for function in ["add", "remove", "add"] {
        let mut args = vec![owned_arg(table), pure_arg(&1u64)];
        if function == "add" {
            args.push(pure_arg(&7u64));
        }
        let (fx, checkpoint) = execute(
            &onchain,
            builder(gas)
                .move_call_with_type_args(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    "table",
                    function,
                    key_and_value_types(),
                    args,
                )
                .build(),
        )
        .await;
        gas = output_ref(&fx, gas.0);
        table = output_ref(&fx, table.0);
        let id = if function == "add" {
            let added = created_object(&fx);
            changes.live(checkpoint, added.0, added.1);
            added.0
        } else {
            let deleted = fx.deleted();
            assert_eq!(deleted.len(), 1, "remove deletes the field: {deleted:?}");
            changes.removed(
                checkpoint,
                deleted[0].0,
                deleted[0].1,
                object_removal::Kind::Deleted,
                *fx.transaction_digest(),
            );
            deleted[0].0
        };
        assert_eq!(*field.get_or_insert(id), id, "the field keeps its ID");
    }

    // One more checkpoint, after every change, so the reads after the last change are at a
    // checkpoint both servers serve.
    let (_, tip) = execute(&onchain, builder(gas).transfer_sui(Some(1), sender).build()).await;
    onchain.wait_for_rpc_index_ready().await;
    offchain
        .wait_for_kv_rpc(tip, Duration::from_secs(60))
        .await
        .unwrap_or_else(|_| panic!("kv-rpc serves checkpoint {tip}"));

    // Every difference between the servers' answers, reported together at the end.
    let mut mismatches = vec![];
    let mut reads = 0;
    let mut all_requests = vec![];
    let mut all_answers = vec![];
    for id in changes.ids() {
        let history = changes.history(id);
        let checkpoints: BTreeSet<u64> = history
            .iter()
            .flat_map(|change| {
                [
                    change.checkpoint.checked_sub(1),
                    Some(change.checkpoint),
                    Some(change.checkpoint + 1),
                ]
            })
            .flatten()
            .filter(|&checkpoint| checkpoint <= tip)
            .collect();
        let versions: BTreeSet<u64> = history
            .iter()
            .flat_map(|change| [change.version - 1, change.version, change.version + 1])
            .collect();
        let bounds = std::iter::once(None).chain(checkpoints.iter().copied().map(Some));

        let mut cases = vec![(
            object_request(id, None, None, None),
            expected(history.last(), None),
        )];
        for &checkpoint in &checkpoints {
            let at = history
                .iter()
                .rfind(|change| change.checkpoint <= checkpoint);
            cases.push((
                object_request(id, None, None, Some(checkpoint)),
                expected(at, None),
            ));
        }
        for at_checkpoint in bounds {
            for &version in &versions {
                let exact = history.iter().find(|change| change.version == version);
                cases.push((
                    object_request(id, Some(version), None, at_checkpoint),
                    expected(exact, at_checkpoint),
                ));
            }
            for &max_version in versions.iter().chain([&u64::MAX]) {
                let at_most = history.iter().rfind(|change| change.version <= max_version);
                cases.push((
                    object_request(id, None, Some(max_version), at_checkpoint),
                    expected(at_most, at_checkpoint),
                ));
            }
        }

        for (req, expected) in cases {
            let context = describe(&req);
            let from_fullnode = get_object(&mut fullnode, req.clone()).await;
            let from_kv_rpc = get_object(&mut kv_rpc, req.clone()).await;
            assert_expected(&from_kv_rpc, id, &expected, &format!("kv-rpc {context}"));
            compare(
                &mut mismatches,
                format!("GetObject {context}"),
                &from_fullnode,
                &from_kv_rpc,
            );
            reads += 1;
            all_requests.push(req);
            all_answers.push(from_kv_rpc);
        }

        // Listing every version, in both orders, with and without removal transactions.
        let mut listings = vec![];
        for with_transactions in [true, false] {
            let ascending = model_items(&history, with_transactions);
            let mut descending = ascending.clone();
            descending.reverse();
            listings.push((Ordering::Ascending, with_transactions, ascending));
            listings.push((Ordering::Descending, with_transactions, descending));
        }
        for (ordering, with_transactions, model) in &listings {
            let (ordering, with_transactions) = (*ordering, *with_transactions);
            let context = format!("{id} {ordering:?} with_transactions {with_transactions}");
            let req = list_request(id, ordering, None, None, None, with_transactions);
            let from_fullnode = list_object_versions(&mut fullnode, req.clone()).await;
            let from_kv_rpc = list_object_versions(&mut kv_rpc, req).await;
            compare(
                &mut mismatches,
                format!("ListObjectVersions {context}"),
                &from_fullnode,
                &from_kv_rpc,
            );
            assert_eq!(&listed_items(&from_kv_rpc, &context), model, "{context}");

            // Bounded by each checkpoint around a change.
            for &checkpoint in &checkpoints {
                let context = format!("{context} at_checkpoint {checkpoint}");
                let req = list_request(
                    id,
                    ordering,
                    None,
                    None,
                    Some(checkpoint),
                    with_transactions,
                );
                let from_fullnode = list_object_versions(&mut fullnode, req.clone()).await;
                let from_kv_rpc = list_object_versions(&mut kv_rpc, req).await;
                compare(
                    &mut mismatches,
                    format!("ListObjectVersions {context}"),
                    &from_fullnode,
                    &from_kv_rpc,
                );
                let mut bounded = model_items(
                    &history
                        .iter()
                        .filter(|change| change.checkpoint <= checkpoint)
                        .copied()
                        .collect::<Vec<_>>(),
                    with_transactions,
                );
                if ordering == Ordering::Descending {
                    bounded.reverse();
                }
                assert_eq!(listed_items(&from_kv_rpc, &context), bounded, "{context}");
            }

            // In pages, each resumed from the cursor that ended the previous page, as the other
            // server minted it, starting from either server.
            for first_from_fullnode in [true, false] {
                let mut from_fullnode_next = first_from_fullnode;
                let mut cursor = None;
                let mut paged = vec![];
                loop {
                    let context = format!(
                        "{context} page {} from {}",
                        paged.len(),
                        if from_fullnode_next {
                            "fullnode"
                        } else {
                            "kv-rpc"
                        },
                    );
                    let req = list_request(
                        id,
                        ordering,
                        Some(PAGE_SIZE),
                        cursor,
                        None,
                        with_transactions,
                    );
                    let from_fullnode = list_object_versions(&mut fullnode, req.clone()).await;
                    let from_kv_rpc = list_object_versions(&mut kv_rpc, req).await;
                    compare(
                        &mut mismatches,
                        format!("ListObjectVersions {context}"),
                        &from_fullnode,
                        &from_kv_rpc,
                    );
                    let page = if from_fullnode_next {
                        from_fullnode
                    } else {
                        from_kv_rpc
                    };
                    let items = listed_items(&page, &context);
                    assert!(items.len() <= PAGE_SIZE as usize, "{context}: {items:?}");
                    paged.extend(items);
                    if end_reason(&page, &context) != QueryEndReason::ItemLimit {
                        break;
                    }
                    cursor = last_cursor(&page);
                    assert!(cursor.is_some(), "{context}: a full page has a cursor");
                    from_fullnode_next = !from_fullnode_next;
                }
                assert_eq!(&paged, model, "{context} paged");
            }
        }
    }

    // The same reads, mixed across objects and selectors in batches, answer item by item what
    // `GetObject` answered on each server.
    let mut order: Vec<usize> = (0..all_requests.len()).collect();
    // Interleave the objects' reads deterministically.
    order.sort_by_key(|&i| (i * 7919) % all_requests.len());
    for chunk in order.chunks(BATCH_SIZE) {
        let requests: Vec<GetObjectRequest> =
            chunk.iter().map(|&i| all_requests[i].clone()).collect();
        let from_fullnode = batch_get_objects(&mut fullnode, requests.clone()).await;
        let from_kv_rpc = batch_get_objects(&mut kv_rpc, requests.clone()).await;
        for (((req, from_fullnode), from_kv_rpc), &i) in requests
            .iter()
            .zip_eq(from_fullnode)
            .zip_eq(from_kv_rpc)
            .zip_eq(chunk)
        {
            let context = describe(req);
            assert_eq!(
                from_kv_rpc, all_answers[i],
                "BatchGetObjects {context}: kv-rpc's batch item (left) differs from its GetObject"
            );
            compare(
                &mut mismatches,
                format!("BatchGetObjects {context}"),
                &from_fullnode,
                &from_kv_rpc,
            );
        }
    }

    eprintln!("{reads} reads compared");
    assert!(
        mismatches.is_empty(),
        "{} answers differ between the fullnode and kv-rpc:\n{}",
        mismatches.len(),
        mismatches.join("\n"),
    );
}
