// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::TryStreamExt;
use sui_kvstore::EpochData;
use sui_kvstore::ObjectStatus;
use sui_kvstore::tables::epochs::col;
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Epoch;
use sui_rpc::proto::sui::rpc::v2::ProtocolConfig as RpcProtocolConfig;
use sui_rpc::proto::sui::rpc::v2::{GetEpochRequest, GetEpochResponse};
use sui_rpc_api::grpc::v2::ledger_service::set_epoch_end_fields;
use sui_rpc_api::{
    EpochNotFoundError, ErrorReason, NotServedHereCause, RpcError,
    grpc::v2::protocol_config_to_proto,
    not_served_here,
    proto::{google::rpc::bad_request::FieldViolation, timestamp_ms_to_proto},
};
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;
use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::object::Object;
use sui_types::storage::{EpochInfo, ObjectKey};
use sui_types::sui_system_state::{
    SuiSystemState, SuiSystemStateTrait, get_sui_system_state, get_sui_system_state_wrapper,
    sui_system_state_inner_id,
};

use crate::bigtable_client::BigTableClient;
use crate::served_height::ServedHeight;

pub const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::EPOCH;

/// Serve the epoch the request names, or the latest one, as of `served_height`: an epoch that
/// starts above it is not found, and an epoch that ends above it has not ended yet, so its end
/// fields are left out even when the indexer has already written them.
///
/// `system_state` is the epoch-start snapshot for an epoch below the served one. For the served
/// epoch it is the state at the served checkpoint, read from the object versions index when
/// `object_at_checkpoint_enabled`, or at the checkpoint before it when the served checkpoint is the
/// epoch's last: the change-epoch transaction there has already turned the state into the next
/// epoch's. `committee` comes from the same state when `system_state` is read too, and from the
/// epoch-start snapshot otherwise; the committee does not change within an epoch.
pub async fn get_epoch(
    client: BigTableClient,
    object_at_checkpoint_enabled: bool,
    served_height: ServedHeight,
    request: GetEpochRequest,
    chain: Chain,
) -> sui_rpc_api::Result<GetEpochResponse> {
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

    let epoch = request.epoch.unwrap_or(served_height.epoch);
    if epoch > served_height.epoch {
        return Err(EpochNotFoundError::new(epoch).into());
    }
    // The served epoch's `system_state` is read at the served height, and `committee` comes from
    // it, so the first read fetches the snapshot only for an epoch below the served one, or for
    // `committee` without `system_state`.
    let reads_state_at_served_height =
        epoch == served_height.epoch && read_mask.contains(Epoch::SYSTEM_STATE_FIELD.name);
    let columns = epoch_columns(&read_mask, !reads_state_at_served_height);
    let epoch_future = async { client.get_epochs_filtered(&[epoch], Some(&columns)).await };
    let transaction_start_future = async {
        if !read_mask.contains(Epoch::TRANSACTION_START_INCLUSIVE_FIELD.name) {
            return Ok(None);
        }
        let Some(previous) = epoch.checked_sub(1) else {
            return Ok(Some(0));
        };
        // The previous epoch ended below this epoch's first checkpoint, so at or below the served
        // height.
        let previous = client
            .get_epochs_filtered(&[previous], Some(&[col::EPOCH, col::TX_HI]))
            .await?
            .pop();
        Ok::<_, RpcError>(previous.and_then(|previous| previous.tx_hi))
    };
    let (mut epochs, transaction_start_inclusive) =
        tokio::try_join!(epoch_future, transaction_start_future)?;
    let mut epoch_info = epochs.pop().ok_or_else(|| EpochNotFoundError::new(epoch))?;
    if epoch_info
        .end_checkpoint
        .is_some_and(|end_checkpoint| end_checkpoint > served_height.checkpoint)
    {
        epoch_info.end_checkpoint = None;
        epoch_info.end_timestamp_ms = None;
    }
    if read_mask.contains(Epoch::EPOCH_FIELD.name) {
        message.epoch = epoch_info.epoch;
    }
    if read_mask.contains(Epoch::FIRST_CHECKPOINT_FIELD.name) {
        message.first_checkpoint = epoch_info.start_checkpoint;
    }
    if read_mask.contains(Epoch::LAST_CHECKPOINT_FIELD.name) {
        message.last_checkpoint = epoch_info.end_checkpoint;
    }
    if read_mask.contains(Epoch::START_FIELD.name) {
        message.start = epoch_info.start_timestamp_ms.map(timestamp_ms_to_proto);
    }
    if read_mask.contains(Epoch::END_FIELD.name) {
        message.end = epoch_info.end_timestamp_ms.map(timestamp_ms_to_proto);
    }
    if read_mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name) {
        message.reference_gas_price = epoch_info.reference_gas_price;
    }
    if let (Some(submask), Some(version)) = (
        read_mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name),
        epoch_info.protocol_version,
    ) {
        let protocol_config = ProtocolConfig::get_for_version_if_supported(version.into(), chain);
        message.protocol_config = protocol_config.map(|config| {
            RpcProtocolConfig::merge_from(protocol_config_to_proto(config), &submask)
        });
    }
    if epoch_info.end_checkpoint.is_some() {
        set_epoch_end_fields(&mut message, &read_mask, &epoch_end(&epoch_info)?);
    }
    message.transaction_start_inclusive = transaction_start_inclusive;
    let system_state = if reads_state_at_served_height {
        // Epoch 0's last checkpoint is never the genesis checkpoint.
        let checkpoint = if epoch_info.end_checkpoint == Some(served_height.checkpoint) {
            served_height.checkpoint - 1
        } else {
            served_height.checkpoint
        };
        Some(live_system_state(&client, object_at_checkpoint_enabled, checkpoint).await?)
    } else {
        epoch_info.system_state.take()
    };
    if read_mask.contains(Epoch::COMMITTEE_FIELD.name) {
        message.committee = system_state.as_ref().map(|system_state| {
            system_state
                .get_current_epoch_committee()
                .committee()
                .clone()
                .into()
        });
    }
    if read_mask.contains(Epoch::SYSTEM_STATE_FIELD.name) {
        message.system_state = system_state.map(|system_state| Box::new(system_state.into()));
    }
    Ok(GetEpochResponse::new(message))
}

/// The end-of-epoch columns of `epoch_info`, decoded into the shape `set_epoch_end_fields` reads.
fn epoch_end(epoch_info: &EpochData) -> Result<EpochInfo, RpcError> {
    let epoch_commitments = epoch_info
        .epoch_commitments
        .as_deref()
        .map(bcs::from_bytes::<Vec<CheckpointCommitment>>)
        .transpose()
        .map_err(|e| {
            RpcError::new(
                tonic::Code::Internal,
                format!("decode epoch commitments: {e}"),
            )
        })?;
    Ok(EpochInfo {
        tx_hi: epoch_info.tx_hi,
        safe_mode: epoch_info.safe_mode,
        epoch_commitments,
        total_stake: epoch_info.total_stake,
        storage_fund_balance: epoch_info.storage_fund_balance,
        storage_fund_reinvestment: epoch_info.storage_fund_reinvestment,
        storage_charge: epoch_info.storage_charge,
        storage_rebate: epoch_info.storage_rebate,
        stake_subsidy_amount: epoch_info.stake_subsidy_amount,
        total_gas_fees: epoch_info.total_gas_fees,
        total_stake_rewards_distributed: epoch_info.total_stake_rewards_distributed,
        leftover_storage_fund_inflow: epoch_info.leftover_storage_fund_inflow,
        ..EpochInfo::default()
    })
}

/// The system state at the end of `checkpoint`: the `0x5` wrapper and the inner object its
/// `version` keys, each read at that checkpoint. The version is read too, not taken from the
/// epoch-start snapshot, because the first mutable access to the system state after genesis or a
/// framework upgrade migrates the inner object to a new version mid-epoch. `Unimplemented` /
/// `NOT_SERVED_HERE` (cause `disabled_by_config`) unless `object_at_checkpoint_enabled`.
async fn live_system_state(
    client: &BigTableClient,
    object_at_checkpoint_enabled: bool,
    checkpoint: u64,
) -> Result<SuiSystemState, RpcError> {
    if !object_at_checkpoint_enabled {
        return Err(not_served_here(
            NotServedHereCause::DisabledByConfig,
            "the running epoch's system_state needs at_checkpoint object reads, which are not \
             enabled on this instance",
        ));
    }
    let internal = |message: String| RpcError::new(tonic::Code::Internal, message);
    let mut objects =
        vec![object_at_checkpoint(client, SUI_SYSTEM_STATE_OBJECT_ID, checkpoint).await?];
    let version = get_sui_system_state_wrapper(&objects.as_slice())
        .map_err(|e| internal(format!("decode system state wrapper: {e}")))?
        .version;
    objects
        .push(object_at_checkpoint(client, sui_system_state_inner_id(version), checkpoint).await?);
    get_sui_system_state(&objects.as_slice()).map_err(|e| {
        internal(format!(
            "decode system state at checkpoint {checkpoint}: {e}"
        ))
    })
}

/// The version of `id` live at the end of `checkpoint`, from the version index (a single-row
/// forward read), read from the `objects` table. `Internal` if it is not live there: callers read
/// only objects that always exist.
async fn object_at_checkpoint(
    client: &BigTableClient,
    id: ObjectID,
    checkpoint: u64,
) -> Result<Object, RpcError> {
    let internal = |message: String| RpcError::new(tonic::Code::Internal, message);
    let version = match client
        .get_object_version_at_checkpoint(id, checkpoint)
        .await?
    {
        Some((version, ObjectStatus::Live)) => version,
        other => {
            return Err(internal(format!(
                "object {id} is not live at checkpoint {checkpoint}: {other:?}"
            )));
        }
    };
    // The read runs to its end, so it finishes as completed rather than cut off.
    let rows: Vec<Object> = client
        .get_objects_stream(vec![ObjectKey(id, version)])
        .await?
        .try_collect()
        .await?;
    rows.into_iter().next().ok_or_else(|| {
        internal(format!(
            "object {id} version {version} is not in the objects table"
        ))
    })
}

/// Compute the set of BigTable columns needed for the given read mask. Always includes `ep` and
/// `ec`: the end checkpoint decides whether the epoch has ended at the served height, which every
/// end field and `system_state` depend on. The epoch-start snapshot is read for `committee` or
/// `system_state` only if `reads_snapshot`.
fn epoch_columns(mask: &FieldMaskTree, reads_snapshot: bool) -> Vec<&'static str> {
    let mut columns = vec![col::EPOCH, col::END_CHECKPOINT];

    if mask.contains(Epoch::FIRST_CHECKPOINT_FIELD.name) {
        columns.push(col::START_CHECKPOINT);
    }
    let end_fields = [
        (
            Epoch::TRANSACTION_END_EXCLUSIVE_FIELD.name,
            &[col::TX_HI][..],
        ),
        (Epoch::ENDED_IN_SAFE_MODE_FIELD.name, &[col::SAFE_MODE][..]),
        (
            Epoch::SYSTEM_EPOCH_INFO_FIELD.name,
            &[
                col::TOTAL_STAKE,
                col::STORAGE_FUND_REINVESTMENT,
                col::STORAGE_CHARGE,
                col::STORAGE_REBATE,
                col::STORAGE_FUND_BALANCE,
                col::STAKE_SUBSIDY_AMOUNT,
                col::TOTAL_GAS_FEES,
                col::TOTAL_STAKE_REWARDS_DISTRIBUTED,
                col::LEFTOVER_STORAGE_FUND_INFLOW,
            ][..],
        ),
        (
            Epoch::EPOCH_COMMITMENTS_FIELD.name,
            &[col::EPOCH_COMMITMENTS][..],
        ),
    ];
    for (field, field_columns) in end_fields {
        if mask.contains(field) {
            columns.extend_from_slice(field_columns);
        }
    }
    if mask.contains(Epoch::START_FIELD.name) {
        columns.push(col::START_TIMESTAMP);
    }
    if mask.contains(Epoch::END_FIELD.name) {
        columns.push(col::END_TIMESTAMP);
    }
    if mask.contains(Epoch::REFERENCE_GAS_PRICE_FIELD.name) {
        columns.push(col::REFERENCE_GAS_PRICE);
    }
    if mask.subtree(Epoch::PROTOCOL_CONFIG_FIELD.name).is_some() {
        columns.push(col::PROTOCOL_VERSION);
    }
    if reads_snapshot
        && (mask.contains(Epoch::COMMITTEE_FIELD.name)
            || mask.contains(Epoch::SYSTEM_STATE_FIELD.name))
    {
        columns.push(col::SYSTEM_STATE);
    }

    columns
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use sui_kvstore::BigTableClient as KvStoreClient;
    use sui_kvstore::tables::{epochs, object_versions_by_checkpoint, objects};
    use sui_kvstore::testing::MockBigtableServer;
    use sui_types::base_types::SequenceNumber;
    use sui_types::sui_system_state::mock;

    use super::*;

    fn u64_cell(column: &'static str, value: u64) -> (&'static str, Bytes) {
        (column, Bytes::copy_from_slice(&value.to_be_bytes()))
    }

    const BOUNDS_MASK: &[&str] = &["epoch", "first_checkpoint", "last_checkpoint", "end"];

    async fn try_get(
        client: &KvStoreClient,
        object_at_checkpoint_enabled: bool,
        served_height: ServedHeight,
        epoch: Option<u64>,
        paths: &[&str],
    ) -> sui_rpc_api::Result<Epoch> {
        let mut request = GetEpochRequest::default();
        request.epoch = epoch;
        request.read_mask = Some(FieldMask::from_paths(paths.iter().copied()));
        let client = BigTableClient::new(
            client.clone(),
            8,
            crate::bigtable_client::Metrics::for_testing().0,
            "GetEpoch",
        );
        get_epoch(
            client,
            object_at_checkpoint_enabled,
            served_height,
            request,
            Chain::Unknown,
        )
        .await
        .map(|response| response.epoch.unwrap_or_default())
    }

    async fn get(
        client: &KvStoreClient,
        served_height: ServedHeight,
        epoch: Option<u64>,
        paths: &[&str],
    ) -> Epoch {
        try_get(client, false, served_height, epoch, paths)
            .await
            .expect("get_epoch succeeds")
    }

    /// Assert that `GetEpoch` answers `NOT_FOUND` for `epoch`, as the fullnode does.
    async fn assert_not_found(
        client: &KvStoreClient,
        served_height: ServedHeight,
        epoch: Option<u64>,
        expected_epoch: u64,
    ) {
        let status: tonic::Status = try_get(client, false, served_height, epoch, BOUNDS_MASK)
            .await
            .expect_err("epoch is not found")
            .into();
        assert_eq!(status.code(), tonic::Code::NotFound, "{epoch:?}");
        assert_eq!(
            status.message(),
            format!("Epoch {expected_epoch} not found"),
            "{epoch:?}"
        );
    }

    fn v1(reference_gas_price: u64) -> SuiSystemState {
        let mut inner = mock::sui_system_state_inner_v1();
        inner.reference_gas_price = reference_gas_price;
        SuiSystemState::V1(inner)
    }

    fn v2(reference_gas_price: u64) -> SuiSystemState {
        let mut inner = mock::sui_system_state_inner_v2();
        inner.reference_gas_price = reference_gas_price;
        SuiSystemState::V2(inner)
    }

    /// Record `id` at `version` with `status` in the version index, from `checkpoint`.
    async fn insert_version(
        mock: &MockBigtableServer,
        id: ObjectID,
        version: SequenceNumber,
        status: ObjectStatus,
        checkpoint: u64,
    ) {
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&id, checkpoint),
            object_versions_by_checkpoint::encode(version, status),
        )
        .await;
    }

    /// Write `state`'s `0x5` wrapper and inner object, both live from `checkpoint` at an object
    /// version equal to it.
    async fn insert_system_state(
        mock: &MockBigtableServer,
        state: SuiSystemState,
        checkpoint: u64,
    ) {
        let version = SequenceNumber::from_u64(checkpoint);
        for mut object in mock::system_state_output_objects(state) {
            object
                .data
                .try_as_move_mut()
                .expect("move object")
                .increment_version_to(version);
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), version)),
                objects::encode(&object).unwrap(),
            )
            .await;
            insert_version(mock, object.id(), version, ObjectStatus::Live, checkpoint).await;
        }
    }

    /// The served epoch's `system_state` is the state at the served checkpoint, read through the
    /// version index, never a version written above it, and following the inner object when it
    /// migrates to a new `system_state_version` mid-epoch. At the epoch's last checkpoint it is the
    /// state at the checkpoint before, since the change-epoch transaction there has already
    /// written the next epoch's state. A past epoch's is its epoch-start snapshot. Without
    /// checkpoint-pinned reads the served epoch's `system_state` is `Unimplemented` /
    /// `NOT_SERVED_HERE` with cause `disabled_by_config`.
    #[tokio::test]
    async fn served_epoch_system_state_is_read_at_the_served_checkpoint() {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = KvStoreClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        // Epoch 0 starts on a V1 inner object with reference gas price 1 and ends at checkpoint
        // 15. The system state changes at checkpoint 3, and at checkpoint 12 it migrates to V2:
        // the V1 inner object is deleted and a V2 one created under the new version's key. The
        // change-epoch transaction at checkpoint 15 writes epoch 1's state.
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(0),
            [
                u64_cell(col::EPOCH, 0),
                u64_cell(col::START_CHECKPOINT, 0),
                u64_cell(col::END_CHECKPOINT, 15),
                (
                    col::SYSTEM_STATE,
                    Bytes::from(bcs::to_bytes(&v1(1)).unwrap()),
                ),
            ],
        )
        .await;
        insert_system_state(&mock, v1(100), 3).await;
        insert_system_state(&mock, v2(200), 12).await;
        insert_version(
            &mock,
            sui_system_state_inner_id(1),
            SequenceNumber::from_u64(12),
            ObjectStatus::Deleted,
            12,
        )
        .await;
        insert_system_state(&mock, v2(300), 15).await;

        let at = |checkpoint| ServedHeight {
            checkpoint,
            epoch: 0,
        };
        let reference_gas_price = |epoch: Epoch| {
            epoch
                .system_state
                .expect("system_state")
                .reference_gas_price
        };

        for (checkpoint, expected) in [(10, 100), (12, 200), (14, 200), (15, 200)] {
            let epoch = try_get(&client, true, at(checkpoint), None, &["system_state"])
                .await
                .expect("get_epoch succeeds");
            assert_eq!(
                reference_gas_price(epoch),
                Some(expected),
                "served at checkpoint {checkpoint}",
            );
        }

        // Once epoch 1 is served, epoch 0 is past: its snapshot, without touching the index, and
        // without checkpoint-pinned reads.
        mock.clear_read_rows_calls().await;
        let inside_epoch_1 = ServedHeight {
            checkpoint: 20,
            epoch: 1,
        };
        let past = try_get(&client, true, inside_epoch_1, Some(0), &["system_state"])
            .await
            .expect("get_epoch succeeds");
        assert_eq!(reference_gas_price(past), Some(1));
        assert!(
            mock.read_rows_calls()
                .await
                .iter()
                .all(|call| call.table != object_versions_by_checkpoint::NAME),
            "a past epoch's system_state reads no object versions",
        );
        let past = get(&client, inside_epoch_1, Some(0), &["system_state"]).await;
        assert_eq!(reference_gas_price(past), Some(1));

        for checkpoint in [10, 15] {
            let status = try_get(&client, false, at(checkpoint), None, &["system_state"])
                .await
                .expect_err("system_state needs checkpoint-pinned reads");
            let status = tonic::Status::from(status);
            assert_eq!(
                status.code(),
                tonic::Code::Unimplemented,
                "served at checkpoint {checkpoint}",
            );
            let error_info =
                <sui_rpc::proto::google::rpc::Status as prost::Message>::decode(status.details())
                    .expect("error details decode as google.rpc.Status")
                    .details
                    .iter()
                    .find_map(|detail| {
                        detail
                            .to_msg::<sui_rpc::proto::google::rpc::ErrorInfo>()
                            .ok()
                    })
                    .unwrap_or_else(|| panic!("no ErrorInfo in {status:?}"));
            assert_eq!(error_info.reason, ErrorReason::NotServedHere.as_str_name());
            assert_eq!(error_info.domain, ErrorReason::DOMAIN);
            assert_eq!(
                error_info.metadata,
                [("cause".to_owned(), "disabled_by_config".to_owned())].into(),
                "served at checkpoint {checkpoint}",
            );
        }
    }

    #[tokio::test]
    async fn epochs_are_served_as_of_the_served_height() {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = KvStoreClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        // Epoch 0 covers checkpoints 0..=9 and its end row is written; epoch 1 starts at 10.
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(0),
            [
                u64_cell(col::EPOCH, 0),
                u64_cell(col::START_CHECKPOINT, 0),
                u64_cell(col::END_CHECKPOINT, 9),
                u64_cell(col::END_TIMESTAMP, 9_000),
            ],
        )
        .await;
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(1),
            [u64_cell(col::EPOCH, 1), u64_cell(col::START_CHECKPOINT, 10)],
        )
        .await;

        // Inside epoch 0: latest is epoch 0, which has not ended yet, and epoch 1 does not exist.
        let inside_epoch_0 = ServedHeight {
            checkpoint: 8,
            epoch: 0,
        };
        for epoch in [None, Some(0)] {
            let served = get(&client, inside_epoch_0, epoch, BOUNDS_MASK).await;
            assert_eq!(served.epoch, Some(0), "{epoch:?}");
            assert_eq!(served.first_checkpoint, Some(0), "{epoch:?}");
            assert_eq!(served.last_checkpoint, None, "{epoch:?}");
            assert_eq!(served.end, None, "{epoch:?}");
        }
        // A mask asking only for `end` still hides it: `end_checkpoint` is read to decide.
        let end_only = get(&client, inside_epoch_0, Some(0), &["end"]).await;
        assert_eq!(end_only.end, None);
        assert_not_found(&client, inside_epoch_0, Some(1), 1).await;

        // At epoch 0's last checkpoint, epoch 0 has ended.
        let end_of_epoch_0 = ServedHeight {
            checkpoint: 9,
            epoch: 0,
        };
        let ended = get(&client, end_of_epoch_0, None, BOUNDS_MASK).await;
        assert_eq!(ended.epoch, Some(0));
        assert_eq!(ended.last_checkpoint, Some(9));
        assert_eq!(ended.end, Some(timestamp_ms_to_proto(9_000)));
        let end_only = get(&client, end_of_epoch_0, Some(0), &["end"]).await;
        assert_eq!(end_only.end, Some(timestamp_ms_to_proto(9_000)));

        // Inside epoch 1: latest is epoch 1.
        let inside_epoch_1 = ServedHeight {
            checkpoint: 10,
            epoch: 1,
        };
        let latest = get(&client, inside_epoch_1, None, BOUNDS_MASK).await;
        assert_eq!(latest.epoch, Some(1));
        assert_eq!(latest.first_checkpoint, Some(10));
    }

    /// An epoch at or below the served epoch with no row is not found, whether named or latest.
    #[tokio::test]
    async fn epoch_without_a_row_is_not_found() {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = KvStoreClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(0),
            [
                u64_cell(col::EPOCH, 0),
                u64_cell(col::START_CHECKPOINT, 0),
                u64_cell(col::END_CHECKPOINT, 9),
            ],
        )
        .await;
        let inside_epoch_1 = ServedHeight {
            checkpoint: 10,
            epoch: 1,
        };

        assert_not_found(&client, inside_epoch_1, Some(1), 1).await;
        assert_not_found(&client, inside_epoch_1, None, 1).await;
        assert_eq!(
            get(&client, inside_epoch_1, Some(0), BOUNDS_MASK)
                .await
                .epoch,
            Some(0)
        );
    }

    const END_MASK: &[&str] = &[
        "transaction_start_inclusive",
        "transaction_end_exclusive",
        "ended_in_safe_mode",
        "system_epoch_info",
        "epoch_commitments",
    ];

    /// The end-of-epoch fields appear once the epoch has ended at or below the served height,
    /// with the counters unset for an epoch that ended in safe mode. The first transaction comes
    /// from the previous epoch's end and is served as soon as the epoch is.
    #[tokio::test]
    async fn end_fields_are_served_once_the_epoch_has_ended_at_the_served_height() {
        let mock = MockBigtableServer::new();
        let (addr, _server) = mock.start().await.expect("start mock BigTable");
        let client = KvStoreClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        let commitments = vec![CheckpointCommitment::CheckpointArtifactsDigest(
            sui_types::digests::CheckpointArtifactsDigest::new([7; 32]),
        )];
        // Epoch 0 ends normally at checkpoint 9 after transaction 49; epoch 1 ends in safe mode at
        // checkpoint 19 after transaction 79.
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(0),
            [
                u64_cell(col::EPOCH, 0),
                u64_cell(col::START_CHECKPOINT, 0),
                u64_cell(col::END_CHECKPOINT, 9),
                u64_cell(col::TX_HI, 50),
                (col::SAFE_MODE, Bytes::from_static(&[0])),
                u64_cell(col::TOTAL_STAKE, 1),
                u64_cell(col::STORAGE_FUND_REINVESTMENT, 2),
                u64_cell(col::STORAGE_CHARGE, 3),
                u64_cell(col::STORAGE_REBATE, 4),
                u64_cell(col::STORAGE_FUND_BALANCE, 5),
                u64_cell(col::STAKE_SUBSIDY_AMOUNT, 6),
                u64_cell(col::TOTAL_GAS_FEES, 7),
                u64_cell(col::TOTAL_STAKE_REWARDS_DISTRIBUTED, 8),
                u64_cell(col::LEFTOVER_STORAGE_FUND_INFLOW, 9),
                (
                    col::EPOCH_COMMITMENTS,
                    Bytes::from(bcs::to_bytes(&commitments).unwrap()),
                ),
            ],
        )
        .await;
        mock.insert_row(
            epochs::NAME,
            epochs::encode_key(1),
            [
                u64_cell(col::EPOCH, 1),
                u64_cell(col::START_CHECKPOINT, 10),
                u64_cell(col::END_CHECKPOINT, 19),
                u64_cell(col::TX_HI, 80),
                (col::SAFE_MODE, Bytes::from_static(&[1])),
                (
                    col::EPOCH_COMMITMENTS,
                    Bytes::from(bcs::to_bytes(&Vec::<CheckpointCommitment>::new()).unwrap()),
                ),
            ],
        )
        .await;
        let at = |checkpoint, epoch| ServedHeight { checkpoint, epoch };

        let running = get(&client, at(8, 0), Some(0), END_MASK).await;
        let mut expected = Epoch::default();
        expected.transaction_start_inclusive = Some(0);
        assert_eq!(running, expected);

        let ended = get(&client, at(9, 0), Some(0), END_MASK).await;
        assert_eq!(ended.transaction_start_inclusive, Some(0));
        assert_eq!(ended.transaction_end_exclusive, Some(50));
        assert_eq!(ended.ended_in_safe_mode, Some(false));
        let info = ended.system_epoch_info.expect("system_epoch_info");
        assert_eq!(
            [
                info.total_stake,
                info.storage_fund_reinvestment,
                info.storage_charge,
                info.storage_rebate,
                info.storage_fund_balance,
                info.stake_subsidy_amount,
                info.total_gas_fees,
                info.total_stake_rewards_distributed,
                info.leftover_storage_fund_inflow,
            ],
            [1, 2, 3, 4, 5, 6, 7, 8, 9].map(Some),
        );
        assert_eq!(
            ended.epoch_commitments,
            commitments
                .into_iter()
                .map(Into::into)
                .collect::<Vec<sui_rpc::proto::sui::rpc::v2::CheckpointCommitment>>(),
        );

        let running = get(&client, at(15, 1), Some(1), END_MASK).await;
        let mut expected = Epoch::default();
        expected.transaction_start_inclusive = Some(50);
        assert_eq!(running, expected);

        let safe_mode = get(&client, at(19, 1), Some(1), END_MASK).await;
        assert_eq!(safe_mode.transaction_start_inclusive, Some(50));
        assert_eq!(safe_mode.transaction_end_exclusive, Some(80));
        assert_eq!(safe_mode.ended_in_safe_mode, Some(true));
        assert_eq!(safe_mode.system_epoch_info, None);
        assert!(safe_mode.epoch_commitments.is_empty());
    }
}
