// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use crate::error::EpochNotFoundError;
use crate::reader::StateReader;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::GetEpochResponse;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::SystemEpochInfo;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::EpochId;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::EpochInfo;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;

pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::EPOCH;

/// On a node with rpc indexes the response is pinned at the highest indexed checkpoint H: the
/// latest epoch is H's epoch, an epoch that starts above H is not found, and an epoch that ends
/// above H has not ended yet, so `last_checkpoint`, `end` and the end-of-epoch fields are left out
/// even when the indexes have already stored them. Without rpc indexes, before they have indexed
/// a checkpoint, while their history is still backfilling after a restore, without object
/// version history, or if H's summary is not stored, the response is unpinned and the latest
/// epoch is the live system state's.
///
/// `system_state` is the live state for the live system state's epoch, which is not capped at any
/// checkpoint, and the epoch-start snapshot for any other epoch. The end-of-epoch fields are set
/// exactly when `last_checkpoint` is, from the same rpc-store row.
#[tracing::instrument(skip(service))]
pub fn get_epoch(service: &RpcService, request: GetEpochRequest) -> Result<GetEpochResponse> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Epoch>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let mut message = Epoch::default();

    let current_system_state = service.reader.get_system_state()?;
    let live_epoch = current_system_state.epoch();

    let indexes = service.reader.inner().indexes();
    let pinned_summary = pinned_checkpoint_summary(&service.reader)?;
    let pinned_checkpoint = pinned_summary
        .as_ref()
        .map(|summary| summary.data().sequence_number);
    let latest_epoch = pinned_summary.map_or(live_epoch, |summary| summary.data().epoch);

    let epoch = request.epoch.unwrap_or(latest_epoch);
    if epoch > latest_epoch {
        return Err(EpochNotFoundError::new(epoch).into());
    }

    // Fetch epoch info, if indexing is available.
    let epoch_info = indexes
        .map(|indexes| indexes.get_epoch_info(epoch))
        .transpose()?
        .flatten()
        .map(|epoch_info| as_of_checkpoint(epoch_info, pinned_checkpoint));

    if epoch != live_epoch && epoch_info.is_none() {
        return Err(EpochNotFoundError::new(epoch).into());
    }

    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.set_epoch(epoch);
    }

    // The live epoch's protocol config, start and reference gas price come from the live state;
    // any other epoch's come from its rpc-store row below.
    if epoch == live_epoch {
        let system_state = &current_system_state;
        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD) {
            let chain = service.reader.inner().get_chain_identifier()?.chain();
            let config = get_protocol_config(system_state.protocol_version(), chain)?;

            message.set_protocol_config(ProtocolConfig::merge_from(config, &submask));
        }

        if read_mask.contains(Epoch::START_FIELD) {
            message.set_start(timestamp_ms_to_proto(
                system_state.epoch_start_timestamp_ms(),
            ));
        }

        if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD) {
            message.set_reference_gas_price(system_state.reference_gas_price());
        }
    }

    if let Some(epoch_info) = &epoch_info {
        if read_mask.contains(Epoch::FIRST_CHECKPOINT_FIELD) {
            message.first_checkpoint = epoch_info.start_checkpoint;
        }

        if read_mask.contains(Epoch::LAST_CHECKPOINT_FIELD) {
            message.last_checkpoint = epoch_info.end_checkpoint;
        }

        if read_mask.contains(Epoch::START_FIELD) && message.start.is_none() {
            message.start = epoch_info.start_timestamp_ms.map(timestamp_ms_to_proto);
        }

        if read_mask.contains(Epoch::END_FIELD) {
            message.end = epoch_info.end_timestamp_ms.map(timestamp_ms_to_proto);
        }

        if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name)
            && message.reference_gas_price.is_none()
        {
            message.reference_gas_price = epoch_info.reference_gas_price;
        }

        if let Some(submask) = read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name)
            && message.protocol_config.is_none()
        {
            let chain = service.reader.inner().get_chain_identifier()?.chain();
            let protocol_config = epoch_info
                .protocol_version
                .map(|version| get_protocol_config(version, chain))
                .transpose()?;

            message.protocol_config =
                protocol_config.map(|config| ProtocolConfig::merge_from(config, &submask));
        }

        if epoch_info.end_checkpoint.is_some() {
            set_epoch_end_fields(&mut message, &read_mask, epoch_info);
        }
    }

    if read_mask.contains(Epoch::TRANSACTION_START_INCLUSIVE_FIELD) {
        message.transaction_start_inclusive = match epoch.checked_sub(1) {
            None => Some(0),
            Some(previous) => indexes
                .map(|indexes| indexes.get_epoch_info(previous))
                .transpose()?
                .flatten()
                .and_then(|info| info.tx_hi),
        };
    }

    if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
        message.committee = Some(
            service
                .reader
                .get_committee(epoch)
                .ok_or_else(|| CommitteeNotFoundError::new(epoch))?
                .into(),
        );
    }

    if read_mask.contains(Epoch::SYSTEM_STATE_FIELD) {
        let system_state = if epoch == live_epoch {
            Some(current_system_state)
        } else {
            epoch_info.and_then(epoch_start_system_state)
        };
        message.system_state = system_state.map(|system_state| Box::new(system_state.into()));
    }

    Ok(GetEpochResponse::new(message))
}

/// The summary of the highest indexed checkpoint H, which the response is pinned at. `None`, so
/// the response is unpinned, without rpc indexes or object version history, while the indexes'
/// history is still backfilling after a restore, before they have indexed a checkpoint, or if
/// H's summary is not stored.
fn pinned_checkpoint_summary(reader: &StateReader) -> Result<Option<VerifiedCheckpoint>> {
    let Some(indexes) = reader.inner().indexes() else {
        return Ok(None);
    };
    // Object version history is node configuration; checking it first spares a node without it
    // the backfill reads.
    if !reader.inner().retains_object_version_history() || indexes.is_history_backfill_pending()? {
        return Ok(None);
    }
    Ok(indexes
        .get_highest_indexed_checkpoint_seq_number()?
        .and_then(|checkpoint| reader.inner().get_checkpoint_by_sequence_number(checkpoint)))
}

/// `epoch_info` as of `pinned_checkpoint`: an epoch that ends above it has not ended yet, so its
/// end checkpoint and timestamp are cleared, and with them every end-of-epoch field.
fn as_of_checkpoint(mut epoch_info: EpochInfo, pinned_checkpoint: Option<u64>) -> EpochInfo {
    if let (Some(end_checkpoint), Some(pinned_checkpoint)) =
        (epoch_info.end_checkpoint, pinned_checkpoint)
        && end_checkpoint > pinned_checkpoint
    {
        epoch_info.end_checkpoint = None;
        epoch_info.end_timestamp_ms = None;
    }
    epoch_info
}

/// The rpc-store row's epoch-start snapshot. A restore that lands mid-epoch seeds the row from
/// the state at the restore point and leaves `start_checkpoint` unset until the epoch's first
/// checkpoint is indexed, so that state is not served as the start state.
fn epoch_start_system_state(epoch_info: EpochInfo) -> Option<SuiSystemState> {
    epoch_info.start_checkpoint?;
    epoch_info.system_state
}

/// Set the end-of-epoch fields `read_mask` selects from `epoch_info`. Call it exactly when the
/// response serves the epoch's `last_checkpoint`, so the end fields appear together with it.
pub fn set_epoch_end_fields(
    message: &mut Epoch,
    read_mask: &FieldMaskTree,
    epoch_info: &EpochInfo,
) {
    if read_mask.contains(Epoch::TRANSACTION_END_EXCLUSIVE_FIELD) {
        message.transaction_end_exclusive = epoch_info.tx_hi;
    }
    if read_mask.contains(Epoch::ENDED_IN_SAFE_MODE_FIELD) {
        message.ended_in_safe_mode = epoch_info.safe_mode;
    }
    if read_mask.contains(Epoch::SYSTEM_EPOCH_INFO_FIELD) {
        let mut info = SystemEpochInfo::default();
        info.total_stake = epoch_info.total_stake;
        info.storage_fund_reinvestment = epoch_info.storage_fund_reinvestment;
        info.storage_charge = epoch_info.storage_charge;
        info.storage_rebate = epoch_info.storage_rebate;
        info.storage_fund_balance = epoch_info.storage_fund_balance;
        info.stake_subsidy_amount = epoch_info.stake_subsidy_amount;
        info.total_gas_fees = epoch_info.total_gas_fees;
        info.total_stake_rewards_distributed = epoch_info.total_stake_rewards_distributed;
        info.leftover_storage_fund_inflow = epoch_info.leftover_storage_fund_inflow;
        // An epoch that ended in safe mode emitted no `SystemEpochInfoEvent`, so every counter
        // is unset and so is the message.
        message.system_epoch_info = (info != SystemEpochInfo::default()).then_some(info);
    }
    if read_mask.contains(Epoch::EPOCH_COMMITMENTS_FIELD) {
        message.epoch_commitments = epoch_info
            .epoch_commitments
            .iter()
            .flatten()
            .cloned()
            .map(Into::into)
            .collect();
    }
}

#[derive(Debug)]
pub struct CommitteeNotFoundError {
    epoch: EpochId,
}

impl CommitteeNotFoundError {
    pub fn new(epoch: EpochId) -> Self {
        Self { epoch }
    }
}

impl std::fmt::Display for CommitteeNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Committee for epoch {} not found", self.epoch)
    }
}

impl std::error::Error for CommitteeNotFoundError {}

impl From<CommitteeNotFoundError> for crate::RpcError {
    fn from(value: CommitteeNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

#[derive(Debug)]
struct ProtocolVersionNotFoundError {
    version: u64,
}

impl ProtocolVersionNotFoundError {
    pub fn new(version: u64) -> Self {
        Self { version }
    }
}

impl std::fmt::Display for ProtocolVersionNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Protocol version {} not found", self.version)
    }
}

impl std::error::Error for ProtocolVersionNotFoundError {}

impl From<ProtocolVersionNotFoundError> for crate::RpcError {
    fn from(value: ProtocolVersionNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

fn get_protocol_config(
    version: u64,
    chain: sui_protocol_config::Chain,
) -> Result<ProtocolConfig, ProtocolVersionNotFoundError> {
    let config =
        sui_protocol_config::ProtocolConfig::get_for_version_if_supported(version.into(), chain)
            .ok_or_else(|| ProtocolVersionNotFoundError::new(version))?;
    Ok(protocol_config_to_proto(config))
}

pub fn protocol_config_to_proto(config: sui_protocol_config::ProtocolConfig) -> ProtocolConfig {
    use prost_types::value::Kind;

    let mut message = ProtocolConfig::default();
    message.set_protocol_version(config.version.as_u64());

    // Set deprecated feature flags to the exact feature_map the protocol config gives us
    message.set_feature_flags(config.feature_map());

    // Load configs (today this is just the `attributes`), rendered to a `Value`. Render emits
    // explicit `NullValue`s for fields unset at this protocol version; filter them out so the
    // public `configs` map only carries values that are actually configured.
    let mut configs = config
        .render::<prost_types::Value>(&mut mysten_common::rpc_format::Unmetered)
        .expect("render to prost Value should succeed")
        .into_iter()
        // filter out NULLs
        .filter(|(_, v)| !matches!(v.kind, None | Some(Kind::NullValue(_))))
        .collect::<std::collections::BTreeMap<_, _>>();

    // For backwards compatibility, render attributes to strings, complex types are json
    // stringified
    message.set_attributes(
        configs
            .iter()
            .filter_map(|(k, v)| match &v.kind {
                Some(Kind::NullValue(_)) => None,
                Some(Kind::NumberValue(n)) => Some((k.to_owned(), n.to_string())),
                Some(Kind::StringValue(s)) => Some((k.to_owned(), s.to_owned())),
                Some(Kind::BoolValue(b)) => Some((k.to_owned(), b.to_string())),
                Some(Kind::StructValue(s)) => Some((
                    k.to_owned(),
                    serde_json::to_string(&sui_rpc::_serde::StructSerializer(s)).unwrap(),
                )),
                Some(Kind::ListValue(list)) => Some((
                    k.to_owned(),
                    serde_json::to_string(&sui_rpc::_serde::ListValueSerializer(list)).unwrap(),
                )),
                None => None,
            })
            .collect(),
    );

    // Convert feature flags to a `Value` then merge with other attributes
    for (k, v) in config
        .feature_map()
        .into_iter()
        .map(|(key, value)| (key, prost_types::Value::from(value)))
    {
        let old = configs.insert(k, v);

        debug_assert!(
            old.is_none(),
            "feature flags and attributes can't have keys which are the same"
        );
    }

    // Set the joined set of attributes and feature flags
    message.set_configs(configs);

    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ended_at(end_checkpoint: u64) -> EpochInfo {
        EpochInfo {
            start_checkpoint: Some(0),
            end_checkpoint: Some(end_checkpoint),
            end_timestamp_ms: Some(1_000),
            ..EpochInfo::default()
        }
    }

    /// An epoch has ended at the pinned checkpoint once its last checkpoint is at or below it;
    /// one checkpoint earlier it is still running, so its end and the end fields are left out.
    /// An unpinned read serves the stored end.
    #[test]
    fn epoch_end_is_served_once_it_is_at_or_below_the_pinned_checkpoint() {
        let ended = as_of_checkpoint(ended_at(9), Some(9));
        assert_eq!(ended.end_checkpoint, Some(9));
        assert_eq!(ended.end_timestamp_ms, Some(1_000));

        let running = as_of_checkpoint(ended_at(9), Some(8));
        assert_eq!(running.end_checkpoint, None);
        assert_eq!(running.end_timestamp_ms, None);
        assert_eq!(running.start_checkpoint, Some(0));

        let unpinned = as_of_checkpoint(ended_at(9), None);
        assert_eq!(unpinned.end_checkpoint, Some(9));
        assert_eq!(unpinned.end_timestamp_ms, Some(1_000));
    }

    /// Stores the summary of `checkpoint`, in `epoch`, in the rpc-store at `db`.
    fn store_checkpoint_summary(db: &sui_consistent_store::Db, checkpoint: u64, epoch: u64) {
        use sui_rpc_store::schema::checkpoint_summary;

        let summary = sui_types::messages_checkpoint::CheckpointSummary {
            epoch,
            sequence_number: checkpoint,
            network_total_transactions: 0,
            content_digest: sui_types::digests::CheckpointContentsDigest::new([0; 32]),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: sui_types::gas::GasCostSummary::default(),
            timestamp_ms: 0,
            checkpoint_commitments: vec![],
            end_of_epoch_data: None,
            version_specific_data: vec![],
        };
        let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo {
            epoch,
            signature: sui_types::crypto::AggregateAuthoritySignature::default(),
            signers_map: roaring::RoaringBitmap::new(),
        };
        let summaries = sui_consistent_store::DbMap::<
            checkpoint_summary::Key,
            checkpoint_summary::Value,
        >::new(db.clone(), checkpoint_summary::NAME)
        .unwrap();
        let mut batch = db.batch();
        batch
            .put(
                &summaries,
                &sui_rpc_store::schema::primitives::U64Be(checkpoint),
                &checkpoint_summary::store(&summary, &signature),
            )
            .unwrap();
        batch.commit().unwrap();
    }

    /// While a restore's history backfill is pending, the response is unpinned even though H's
    /// summary is stored: the latest epoch is the live one, and an epoch that ended above H keeps
    /// its stored end. Once history reaches the restore checkpoint, the response is pinned at H.
    #[test]
    fn response_is_unpinned_while_restore_backfill_is_pending() {
        use crate::reader::tests::commit_every_pipeline;
        use crate::reader::tests::record_restore_state;
        use crate::reader::tests::rpc_store_state_reader;
        use sui_consistent_store::restore_state;

        let (_dir, db, reader) = rpc_store_state_reader();
        store_checkpoint_summary(&db, 10, 1);
        store_checkpoint_summary(&db, 20, 2);
        // A restore of the live cohort at 20, whose history has backfilled to 10.
        record_restore_state(
            &db,
            sui_rpc_store::LIVE_COHORT,
            restore_state::State::Complete(restore_state::Complete { restored_at: 20 }),
        );
        commit_every_pipeline(&db, 10);

        // The pinned checkpoint H and its epoch, the latest epoch a pinned response serves.
        let pinned = || {
            pinned_checkpoint_summary(&reader)
                .unwrap()
                .map(|summary| (summary.data().sequence_number, summary.data().epoch))
        };
        assert_eq!(pinned(), None);
        let ended = as_of_checkpoint(ended_at(15), pinned().map(|(checkpoint, _)| checkpoint));
        assert_eq!(ended.end_checkpoint, Some(15));
        assert_eq!(ended.end_timestamp_ms, Some(1_000));

        commit_every_pipeline(&db, 20);
        assert_eq!(pinned(), Some((20, 2)));
        let running = as_of_checkpoint(ended_at(25), pinned().map(|(checkpoint, _)| checkpoint));
        assert_eq!(running.end_checkpoint, None);
    }
}
