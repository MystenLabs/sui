// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Module for conversions from sui-core types to rpc protos

use crate::crypto::SuiSignature;
use crate::message_envelope::Message as _;
use fastcrypto::traits::ToFromBytes;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::TryFromProtoError;
use sui_rpc::proto::sui::rpc::v2::*;

//
// CheckpointSummary
//

impl Merge<&crate::full_checkpoint_content::Checkpoint> for Checkpoint {
    fn merge(&mut self, source: &crate::full_checkpoint_content::Checkpoint, mask: &FieldMaskTree) {
        let sequence_number = source.summary.sequence_number;
        let timestamp_ms = source.summary.timestamp_ms;

        let summary = source.summary.data();
        let signature = source.summary.auth_sig();

        self.merge(summary, mask);
        self.merge(signature.clone(), mask);

        if mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            self.merge(&source.contents, mask);
        }

        if let Some(submask) = mask
            .subtree(Checkpoint::OBJECTS_FIELD)
            .and_then(|submask| submask.subtree(ObjectSet::OBJECTS_FIELD))
        {
            let set = source
                .object_set
                .iter()
                .map(|o| sui_rpc::proto::sui::rpc::v2::Object::merge_from(o, &submask))
                .collect();
            self.objects = Some(ObjectSet::default().with_objects(set));
        }

        if let Some(submask) = mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
            self.transactions = source
                .transactions
                .iter()
                .map(|t| {
                    let mut transaction = ExecutedTransaction::merge_from(t, &submask);
                    transaction.checkpoint = submask
                        .contains(ExecutedTransaction::CHECKPOINT_FIELD)
                        .then_some(sequence_number);
                    transaction.timestamp = submask
                        .contains(ExecutedTransaction::TIMESTAMP_FIELD)
                        .then(|| sui_rpc::proto::timestamp_ms_to_proto(timestamp_ms));
                    transaction
                })
                .collect();
        }
    }
}

impl Merge<&crate::full_checkpoint_content::ExecutedTransaction> for ExecutedTransaction {
    fn merge(
        &mut self,
        source: &crate::full_checkpoint_content::ExecutedTransaction,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(ExecutedTransaction::DIGEST_FIELD) {
            self.digest = Some(source.transaction.digest().to_string());
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD) {
            self.transaction = Some(Transaction::merge_from(&source.transaction, &submask));
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD) {
            self.signatures = source
                .signatures
                .iter()
                .map(|s| UserSignature::merge_from(s, &submask))
                .collect();
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD) {
            let mut effects = TransactionEffects::merge_from(&source.effects, &submask);
            if submask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD) {
                effects.set_unchanged_loaded_runtime_objects(
                    source
                        .unchanged_loaded_runtime_objects
                        .iter()
                        .map(Into::into)
                        .collect(),
                );
            }
            self.effects = Some(effects);
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD) {
            self.events = source
                .events
                .as_ref()
                .map(|events| TransactionEvents::merge_from(events, &submask));
        }
    }
}

impl TryFrom<&Checkpoint> for crate::full_checkpoint_content::Checkpoint {
    type Error = TryFromProtoError;

    fn try_from(checkpoint: &Checkpoint) -> Result<Self, Self::Error> {
        let summary = checkpoint
            .summary()
            .bcs()
            .deserialize()
            .map_err(|e| TryFromProtoError::invalid("summary.bcs", e))?;

        let signature =
            crate::crypto::AuthorityStrongQuorumSignInfo::try_from(checkpoint.signature())?;

        let summary = crate::messages_checkpoint::CertifiedCheckpointSummary::new_from_data_and_sig(
            summary, signature,
        );

        let contents = checkpoint
            .contents()
            .bcs()
            .deserialize()
            .map_err(|e| TryFromProtoError::invalid("contents.bcs", e))?;

        let transactions = checkpoint
            .transactions()
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let object_set = checkpoint.objects().try_into()?;

        Ok(Self {
            summary,
            contents,
            transactions,
            object_set,
        })
    }
}

impl TryFrom<&ExecutedTransaction> for crate::full_checkpoint_content::ExecutedTransaction {
    type Error = TryFromProtoError;

    fn try_from(value: &ExecutedTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: value
                .transaction()
                .bcs()
                .deserialize()
                .map_err(|e| TryFromProtoError::invalid("transaction.bcs", e))?,
            signatures: value
                .signatures()
                .iter()
                .map(|sig| {
                    crate::signature::GenericSignature::from_bytes(sig.bcs().value())
                        .map_err(|e| TryFromProtoError::invalid("signature.bcs", e))
                })
                .collect::<Result<_, _>>()?,
            effects: value
                .effects()
                .bcs()
                .deserialize()
                .map_err(|e| TryFromProtoError::invalid("effects.bcs", e))?,
            events: value
                .events_opt()
                .map(|events| {
                    events
                        .bcs()
                        .deserialize()
                        .map_err(|e| TryFromProtoError::invalid("effects.bcs", e))
                })
                .transpose()?,
            unchanged_loaded_runtime_objects: value
                .effects()
                .unchanged_loaded_runtime_objects()
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<&ObjectReference> for crate::storage::ObjectKey {
    type Error = TryFromProtoError;

    fn try_from(value: &ObjectReference) -> Result<Self, Self::Error> {
        Ok(Self(
            value
                .object_id()
                .parse()
                .map_err(|e| TryFromProtoError::invalid("object_id", e))?,
            value.version().into(),
        ))
    }
}

//
// CheckpointSummary
//

impl From<crate::messages_checkpoint::CheckpointSummary> for CheckpointSummary {
    fn from(summary: crate::messages_checkpoint::CheckpointSummary) -> Self {
        Self::merge_from(summary, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::messages_checkpoint::CheckpointSummary> for CheckpointSummary {
    fn merge(
        &mut self,
        source: crate::messages_checkpoint::CheckpointSummary,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD) {
            let mut bcs = Bcs::serialize(&source).unwrap();
            bcs.name = Some("CheckpointSummary".to_owned());
            self.bcs = Some(bcs);
        }

        if mask.contains(Self::DIGEST_FIELD) {
            self.digest = Some(source.digest().to_string());
        }

        let crate::messages_checkpoint::CheckpointSummary {
            epoch,
            sequence_number,
            network_total_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            checkpoint_commitments,
            end_of_epoch_data,
            version_specific_data,
        } = source;

        if mask.contains(Self::EPOCH_FIELD) {
            self.epoch = Some(epoch);
        }

        if mask.contains(Self::SEQUENCE_NUMBER_FIELD) {
            self.sequence_number = Some(sequence_number);
        }

        if mask.contains(Self::TOTAL_NETWORK_TRANSACTIONS_FIELD) {
            self.total_network_transactions = Some(network_total_transactions);
        }

        if mask.contains(Self::CONTENT_DIGEST_FIELD) {
            self.content_digest = Some(content_digest.to_string());
        }

        if mask.contains(Self::PREVIOUS_DIGEST_FIELD) {
            self.previous_digest = previous_digest.map(|d| d.to_string());
        }

        if mask.contains(Self::EPOCH_ROLLING_GAS_COST_SUMMARY_FIELD) {
            self.epoch_rolling_gas_cost_summary = Some(epoch_rolling_gas_cost_summary.into());
        }

        if mask.contains(Self::TIMESTAMP_FIELD) {
            self.timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(timestamp_ms));
        }

        if mask.contains(Self::COMMITMENTS_FIELD) {
            self.commitments = checkpoint_commitments.into_iter().map(Into::into).collect();
        }

        if mask.contains(Self::END_OF_EPOCH_DATA_FIELD) {
            self.end_of_epoch_data = end_of_epoch_data.map(Into::into);
        }

        if mask.contains(Self::VERSION_SPECIFIC_DATA_FIELD) {
            self.version_specific_data = Some(version_specific_data.into());
        }
    }
}

//
// GasCostSummary
//

impl From<crate::gas::GasCostSummary> for GasCostSummary {
    fn from(
        crate::gas::GasCostSummary {
            computation_cost,
            storage_cost,
            storage_rebate,
            non_refundable_storage_fee,
        }: crate::gas::GasCostSummary,
    ) -> Self {
        let mut message = Self::default();
        message.computation_cost = Some(computation_cost);
        message.storage_cost = Some(storage_cost);
        message.storage_rebate = Some(storage_rebate);
        message.non_refundable_storage_fee = Some(non_refundable_storage_fee);
        message
    }
}

//
// CheckpointCommitment
//

impl From<crate::messages_checkpoint::CheckpointCommitment> for CheckpointCommitment {
    fn from(value: crate::messages_checkpoint::CheckpointCommitment) -> Self {
        use checkpoint_commitment::CheckpointCommitmentKind;

        let mut message = Self::default();

        let kind = match value {
            crate::messages_checkpoint::CheckpointCommitment::ECMHLiveObjectSetDigest(digest) => {
                message.digest = Some(digest.digest.to_string());
                CheckpointCommitmentKind::EcmhLiveObjectSet
            }
            crate::messages_checkpoint::CheckpointCommitment::CheckpointArtifactsDigest(digest) => {
                message.digest = Some(digest.to_string());
                CheckpointCommitmentKind::CheckpointArtifacts
            }
        };

        message.set_kind(kind);
        message
    }
}

//
// EndOfEpochData
//

impl From<crate::messages_checkpoint::EndOfEpochData> for EndOfEpochData {
    fn from(
        crate::messages_checkpoint::EndOfEpochData {
            next_epoch_committee,
            next_epoch_protocol_version,
            epoch_commitments,
        }: crate::messages_checkpoint::EndOfEpochData,
    ) -> Self {
        let mut message = Self::default();

        message.next_epoch_committee = next_epoch_committee
            .into_iter()
            .map(|(name, weight)| {
                let mut member = ValidatorCommitteeMember::default();
                member.public_key = Some(name.0.to_vec().into());
                member.weight = Some(weight);
                member
            })
            .collect();
        message.next_epoch_protocol_version = Some(next_epoch_protocol_version.as_u64());
        message.epoch_commitments = epoch_commitments.into_iter().map(Into::into).collect();

        message
    }
}

//
// CheckpointContents
//

impl From<crate::messages_checkpoint::CheckpointContents> for CheckpointContents {
    fn from(value: crate::messages_checkpoint::CheckpointContents) -> Self {
        Self::merge_from(value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::messages_checkpoint::CheckpointContents> for CheckpointContents {
    fn merge(
        &mut self,
        source: crate::messages_checkpoint::CheckpointContents,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD) {
            let mut bcs = Bcs::serialize(&source).unwrap();
            bcs.name = Some("CheckpointContents".to_owned());
            self.bcs = Some(bcs);
        }

        if mask.contains(Self::DIGEST_FIELD) {
            self.digest = Some(source.digest().to_string());
        }

        if mask.contains(Self::VERSION_FIELD) {
            self.version = Some(1);
        }

        if mask.contains(Self::TRANSACTIONS_FIELD) {
            self.transactions = source
                .into_iter_with_signatures()
                .map(|(digests, sigs)| {
                    let mut info = CheckpointedTransactionInfo::default();
                    info.transaction = Some(digests.transaction.to_string());
                    info.effects = Some(digests.effects.to_string());
                    info.signatures = sigs.into_iter().map(Into::into).collect();
                    info
                })
                .collect();
        }
    }
}

impl Merge<&crate::messages_checkpoint::CheckpointContents> for Checkpoint {
    fn merge(
        &mut self,
        source: &crate::messages_checkpoint::CheckpointContents,
        mask: &FieldMaskTree,
    ) {
        if let Some(submask) = mask.subtree(Self::CONTENTS_FIELD.name) {
            self.contents = Some(CheckpointContents::merge_from(source.to_owned(), &submask));
        }
    }
}

//
// Checkpoint
//

impl Merge<&crate::messages_checkpoint::CheckpointSummary> for Checkpoint {
    fn merge(
        &mut self,
        source: &crate::messages_checkpoint::CheckpointSummary,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(Self::SEQUENCE_NUMBER_FIELD) {
            self.sequence_number = Some(source.sequence_number);
        }

        if mask.contains(Self::DIGEST_FIELD) {
            self.digest = Some(source.digest().to_string());
        }

        if let Some(submask) = mask.subtree(Self::SUMMARY_FIELD) {
            self.summary = Some(CheckpointSummary::merge_from(source.clone(), &submask));
        }
    }
}

impl<const T: bool> Merge<crate::crypto::AuthorityQuorumSignInfo<T>> for Checkpoint {
    fn merge(&mut self, source: crate::crypto::AuthorityQuorumSignInfo<T>, mask: &FieldMaskTree) {
        if mask.contains(Self::SIGNATURE_FIELD) {
            self.signature = Some(source.into());
        }
    }
}

impl Merge<crate::messages_checkpoint::CheckpointContents> for Checkpoint {
    fn merge(
        &mut self,
        source: crate::messages_checkpoint::CheckpointContents,
        mask: &FieldMaskTree,
    ) {
        if let Some(submask) = mask.subtree(Self::CONTENTS_FIELD) {
            self.contents = Some(CheckpointContents::merge_from(source, &submask));
        }
    }
}

//
// Event
//

impl From<crate::event::Event> for Event {
    fn from(value: crate::event::Event) -> Self {
        Self::merge_from(&value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<&crate::event::Event> for Event {
    fn merge(&mut self, source: &crate::event::Event, mask: &FieldMaskTree) {
        if mask.contains(Self::PACKAGE_ID_FIELD) {
            self.package_id = Some(source.package_id.to_canonical_string(true));
        }

        if mask.contains(Self::MODULE_FIELD) {
            self.module = Some(source.transaction_module.to_string());
        }

        if mask.contains(Self::SENDER_FIELD) {
            self.sender = Some(source.sender.to_string());
        }

        if mask.contains(Self::EVENT_TYPE_FIELD) {
            self.event_type = Some(source.type_.to_canonical_string(true));
        }

        if mask.contains(Self::CONTENTS_FIELD) {
            let mut bcs = Bcs::from(source.contents.clone());
            bcs.name = Some(source.type_.to_canonical_string(true));
            self.contents = Some(bcs);
        }
    }
}

//
// TransactionEvents
//

impl From<crate::effects::TransactionEvents> for TransactionEvents {
    fn from(value: crate::effects::TransactionEvents) -> Self {
        Self::merge_from(&value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<&crate::effects::TransactionEvents> for TransactionEvents {
    fn merge(&mut self, source: &crate::effects::TransactionEvents, mask: &FieldMaskTree) {
        if mask.contains(Self::BCS_FIELD) {
            let mut bcs = Bcs::serialize(&source).unwrap();
            bcs.name = Some("TransactionEvents".to_owned());
            self.bcs = Some(bcs);
        }

        if mask.contains(Self::DIGEST_FIELD) {
            self.digest = Some(source.digest().to_string());
        }

        if let Some(events_mask) = mask.subtree(Self::EVENTS_FIELD) {
            self.events = source
                .data
                .iter()
                .map(|event| Event::merge_from(event, &events_mask))
                .collect();
        }
    }
}

//
// SystemState
//

impl From<crate::sui_system_state::SuiSystemState> for SystemState {
    fn from(value: crate::sui_system_state::SuiSystemState) -> Self {
        match value {
            crate::sui_system_state::SuiSystemState::V1(v1) => v1.into(),
            crate::sui_system_state::SuiSystemState::V2(v2) => v2.into(),

            #[allow(unreachable_patterns)]
            _ => Self::default(),
        }
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1>
    for SystemState
{
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1 {
            epoch,
            protocol_version,
            system_state_version,
            validators,
            storage_fund,
            parameters,
            reference_gas_price,
            validator_report_records,
            stake_subsidy,
            safe_mode,
            safe_mode_storage_rewards,
            safe_mode_computation_rewards,
            safe_mode_storage_rebates,
            safe_mode_non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1,
    ) -> Self {
        let validator_report_records = validator_report_records
            .contents
            .into_iter()
            .map(|entry| {
                let mut record = ValidatorReportRecord::default();
                record.reported = Some(entry.key.to_string());
                record.reporters = entry
                    .value
                    .contents
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                record
            })
            .collect();

        let mut message = Self::default();

        message.version = Some(system_state_version);
        message.epoch = Some(epoch);
        message.protocol_version = Some(protocol_version);
        message.validators = Some(validators.into());
        message.storage_fund = Some(storage_fund.into());
        message.parameters = Some(parameters.into());
        message.reference_gas_price = Some(reference_gas_price);
        message.validator_report_records = validator_report_records;
        message.stake_subsidy = Some(stake_subsidy.into());
        message.safe_mode = Some(safe_mode);
        message.safe_mode_storage_rewards = Some(safe_mode_storage_rewards.value());
        message.safe_mode_computation_rewards = Some(safe_mode_computation_rewards.value());
        message.safe_mode_storage_rebates = Some(safe_mode_storage_rebates);
        message.safe_mode_non_refundable_storage_fee = Some(safe_mode_non_refundable_storage_fee);
        message.epoch_start_timestamp_ms = Some(epoch_start_timestamp_ms);
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2>
    for SystemState
{
    fn from(
        crate::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2 {
            epoch,
            protocol_version,
            system_state_version,
            validators,
            storage_fund,
            parameters,
            reference_gas_price,
            validator_report_records,
            stake_subsidy,
            safe_mode,
            safe_mode_storage_rewards,
            safe_mode_computation_rewards,
            safe_mode_storage_rebates,
            safe_mode_non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2,
    ) -> Self {
        let validator_report_records = validator_report_records
            .contents
            .into_iter()
            .map(|entry| {
                let mut record = ValidatorReportRecord::default();
                record.reported = Some(entry.key.to_string());
                record.reporters = entry
                    .value
                    .contents
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                record
            })
            .collect();

        let mut message = Self::default();

        message.version = Some(system_state_version);
        message.epoch = Some(epoch);
        message.protocol_version = Some(protocol_version);
        message.validators = Some(validators.into());
        message.storage_fund = Some(storage_fund.into());
        message.parameters = Some(parameters.into());
        message.reference_gas_price = Some(reference_gas_price);
        message.validator_report_records = validator_report_records;
        message.stake_subsidy = Some(stake_subsidy.into());
        message.safe_mode = Some(safe_mode);
        message.safe_mode_storage_rewards = Some(safe_mode_storage_rewards.value());
        message.safe_mode_computation_rewards = Some(safe_mode_computation_rewards.value());
        message.safe_mode_storage_rebates = Some(safe_mode_storage_rebates);
        message.safe_mode_non_refundable_storage_fee = Some(safe_mode_non_refundable_storage_fee);
        message.epoch_start_timestamp_ms = Some(epoch_start_timestamp_ms);
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::collection_types::Bag> for MoveTable {
    fn from(crate::collection_types::Bag { id, size }: crate::collection_types::Bag) -> Self {
        let mut message = Self::default();
        message.id = Some(id.id.bytes.to_canonical_string(true));
        message.size = Some(size);
        message
    }
}

impl From<crate::collection_types::Table> for MoveTable {
    fn from(crate::collection_types::Table { id, size }: crate::collection_types::Table) -> Self {
        let mut message = Self::default();
        message.id = Some(id.to_canonical_string(true));
        message.size = Some(size);
        message
    }
}

impl From<crate::collection_types::TableVec> for MoveTable {
    fn from(value: crate::collection_types::TableVec) -> Self {
        value.contents.into()
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::StakeSubsidyV1> for StakeSubsidy {
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::StakeSubsidyV1 {
            balance,
            distribution_counter,
            current_distribution_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v1::StakeSubsidyV1,
    ) -> Self {
        let mut message = Self::default();
        message.balance = Some(balance.value());
        message.distribution_counter = Some(distribution_counter);
        message.current_distribution_amount = Some(current_distribution_amount);
        message.stake_subsidy_period_length = Some(stake_subsidy_period_length);
        message.stake_subsidy_decrease_rate = Some(stake_subsidy_decrease_rate.into());
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::SystemParametersV1>
    for SystemParameters
{
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::SystemParametersV1 {
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v1::SystemParametersV1,
    ) -> Self {
        let mut message = Self::default();
        message.epoch_duration_ms = Some(epoch_duration_ms);
        message.stake_subsidy_start_epoch = Some(stake_subsidy_start_epoch);
        message.min_validator_count = None;
        message.max_validator_count = Some(max_validator_count);
        message.min_validator_joining_stake = Some(min_validator_joining_stake);
        message.validator_low_stake_threshold = Some(validator_low_stake_threshold);
        message.validator_very_low_stake_threshold = Some(validator_very_low_stake_threshold);
        message.validator_low_stake_grace_period = Some(validator_low_stake_grace_period);
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v2::SystemParametersV2>
    for SystemParameters
{
    fn from(
        crate::sui_system_state::sui_system_state_inner_v2::SystemParametersV2 {
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            min_validator_count,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v2::SystemParametersV2,
    ) -> Self {
        let mut message = Self::default();
        message.epoch_duration_ms = Some(epoch_duration_ms);
        message.stake_subsidy_start_epoch = Some(stake_subsidy_start_epoch);
        message.min_validator_count = Some(min_validator_count);
        message.max_validator_count = Some(max_validator_count);
        message.min_validator_joining_stake = Some(min_validator_joining_stake);
        message.validator_low_stake_threshold = Some(validator_low_stake_threshold);
        message.validator_very_low_stake_threshold = Some(validator_very_low_stake_threshold);
        message.validator_low_stake_grace_period = Some(validator_low_stake_grace_period);
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::StorageFundV1> for StorageFund {
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::StorageFundV1 {
            total_object_storage_rebates,
            non_refundable_balance,
        }: crate::sui_system_state::sui_system_state_inner_v1::StorageFundV1,
    ) -> Self {
        let mut message = Self::default();
        message.total_object_storage_rebates = Some(total_object_storage_rebates.value());
        message.non_refundable_balance = Some(non_refundable_balance.value());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1> for ValidatorSet {
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1 {
            total_stake,
            active_validators,
            pending_active_validators,
            pending_removals,
            staking_pool_mappings,
            inactive_validators,
            validator_candidates,
            at_risk_validators,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1,
    ) -> Self {
        let at_risk_validators = at_risk_validators
            .contents
            .into_iter()
            .map(|entry| (entry.key.to_string(), entry.value))
            .collect();

        let mut message = Self::default();
        message.total_stake = Some(total_stake);
        message.active_validators = active_validators.into_iter().map(Into::into).collect();
        message.pending_active_validators = Some(pending_active_validators.into());
        message.pending_removals = pending_removals;
        message.staking_pool_mappings = Some(staking_pool_mappings.into());
        message.inactive_validators = Some(inactive_validators.into());
        message.validator_candidates = Some(validator_candidates.into());
        message.at_risk_validators = at_risk_validators;
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::StakingPoolV1> for StakingPool {
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::StakingPoolV1 {
            id,
            activation_epoch,
            deactivation_epoch,
            sui_balance,
            rewards_pool,
            pool_token_balance,
            exchange_rates,
            pending_stake,
            pending_total_sui_withdraw,
            pending_pool_token_withdraw,
            extra_fields,
        }: crate::sui_system_state::sui_system_state_inner_v1::StakingPoolV1,
    ) -> Self {
        let mut message = Self::default();
        message.id = Some(id.to_canonical_string(true));
        message.activation_epoch = activation_epoch;
        message.deactivation_epoch = deactivation_epoch;
        message.sui_balance = Some(sui_balance);
        message.rewards_pool = Some(rewards_pool.value());
        message.pool_token_balance = Some(pool_token_balance);
        message.exchange_rates = Some(exchange_rates.into());
        message.pending_stake = Some(pending_stake);
        message.pending_total_sui_withdraw = Some(pending_total_sui_withdraw);
        message.pending_pool_token_withdraw = Some(pending_pool_token_withdraw);
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::ValidatorV1> for Validator {
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::ValidatorV1 {
            metadata:
                crate::sui_system_state::sui_system_state_inner_v1::ValidatorMetadataV1 {
                    sui_address,
                    protocol_pubkey_bytes,
                    network_pubkey_bytes,
                    worker_pubkey_bytes,
                    proof_of_possession_bytes,
                    name,
                    description,
                    image_url,
                    project_url,
                    net_address,
                    p2p_address,
                    primary_address,
                    worker_address,
                    next_epoch_protocol_pubkey_bytes,
                    next_epoch_proof_of_possession,
                    next_epoch_network_pubkey_bytes,
                    next_epoch_worker_pubkey_bytes,
                    next_epoch_net_address,
                    next_epoch_p2p_address,
                    next_epoch_primary_address,
                    next_epoch_worker_address,
                    extra_fields: metadata_extra_fields,
                },
            voting_power,
            operation_cap_id,
            gas_price,
            staking_pool,
            commission_rate,
            next_epoch_stake,
            next_epoch_gas_price,
            next_epoch_commission_rate,
            extra_fields,
            ..
        }: crate::sui_system_state::sui_system_state_inner_v1::ValidatorV1,
    ) -> Self {
        let mut message = Self::default();
        message.name = Some(name);
        message.address = Some(sui_address.to_string());
        message.description = Some(description);
        message.image_url = Some(image_url);
        message.project_url = Some(project_url);
        message.protocol_public_key = Some(protocol_pubkey_bytes.into());
        message.proof_of_possession = Some(proof_of_possession_bytes.into());
        message.network_public_key = Some(network_pubkey_bytes.into());
        message.worker_public_key = Some(worker_pubkey_bytes.into());
        message.network_address = Some(net_address);
        message.p2p_address = Some(p2p_address);
        message.primary_address = Some(primary_address);
        message.worker_address = Some(worker_address);
        message.next_epoch_protocol_public_key = next_epoch_protocol_pubkey_bytes.map(Into::into);
        message.next_epoch_proof_of_possession = next_epoch_proof_of_possession.map(Into::into);
        message.next_epoch_network_public_key = next_epoch_network_pubkey_bytes.map(Into::into);
        message.next_epoch_worker_public_key = next_epoch_worker_pubkey_bytes.map(Into::into);
        message.next_epoch_network_address = next_epoch_net_address;
        message.next_epoch_p2p_address = next_epoch_p2p_address;
        message.next_epoch_primary_address = next_epoch_primary_address;
        message.next_epoch_worker_address = next_epoch_worker_address;
        message.metadata_extra_fields = Some(metadata_extra_fields.into());
        message.voting_power = Some(voting_power);
        message.operation_cap_id = Some(operation_cap_id.bytes.to_canonical_string(true));
        message.gas_price = Some(gas_price);
        message.staking_pool = Some(staking_pool.into());
        message.commission_rate = Some(commission_rate);
        message.next_epoch_stake = Some(next_epoch_stake);
        message.next_epoch_gas_price = Some(next_epoch_gas_price);
        message.next_epoch_commission_rate = Some(next_epoch_commission_rate);
        message.extra_fields = Some(extra_fields.into());
        message
    }
}

//
// ExecutionStatus
//

impl From<crate::execution_status::ExecutionStatus> for ExecutionStatus {
    fn from(value: crate::execution_status::ExecutionStatus) -> Self {
        let mut message = Self::default();
        match value {
            crate::execution_status::ExecutionStatus::Success => {
                message.success = Some(true);
            }
            crate::execution_status::ExecutionStatus::Failure { error, command } => {
                let description = if let Some(command) = command {
                    format!("{error:?} in command {command}")
                } else {
                    format!("{error:?}")
                };
                let mut error_message = ExecutionError::from(error);
                error_message.command = command.map(|i| i as u64);
                error_message.description = Some(description);

                message.success = Some(false);
                message.error = Some(error_message);
            }
        }

        message
    }
}

//
// ExecutionError
//

fn size_error(size: u64, max_size: u64) -> SizeError {
    let mut message = SizeError::default();
    message.size = Some(size);
    message.max_size = Some(max_size);
    message
}

fn index_error(index: u32, secondary_idx: Option<u32>) -> IndexError {
    let mut message = IndexError::default();
    message.index = Some(index);
    message.subresult = secondary_idx;
    message
}

impl From<crate::execution_status::ExecutionFailureStatus> for ExecutionError {
    fn from(value: crate::execution_status::ExecutionFailureStatus) -> Self {
        use crate::execution_status::ExecutionFailureStatus as E;
        use execution_error::ErrorDetails;
        use execution_error::ExecutionErrorKind;

        let mut message = Self::default();

        let kind = match value {
            E::InsufficientGas => ExecutionErrorKind::InsufficientGas,
            E::InvalidGasObject => ExecutionErrorKind::InvalidGasObject,
            E::InvariantViolation => ExecutionErrorKind::InvariantViolation,
            E::FeatureNotYetSupported => ExecutionErrorKind::FeatureNotYetSupported,
            E::MoveObjectTooBig {
                object_size,
                max_object_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(size_error(
                    object_size,
                    max_object_size,
                )));
                ExecutionErrorKind::ObjectTooBig
            }
            E::MovePackageTooBig {
                object_size,
                max_object_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(size_error(
                    object_size,
                    max_object_size,
                )));
                ExecutionErrorKind::PackageTooBig
            }
            E::CircularObjectOwnership { object } => {
                message.error_details =
                    Some(ErrorDetails::ObjectId(object.to_canonical_string(true)));
                ExecutionErrorKind::CircularObjectOwnership
            }
            E::InsufficientCoinBalance => ExecutionErrorKind::InsufficientCoinBalance,
            E::CoinBalanceOverflow => ExecutionErrorKind::CoinBalanceOverflow,
            E::PublishErrorNonZeroAddress => ExecutionErrorKind::PublishErrorNonZeroAddress,
            E::SuiMoveVerificationError => ExecutionErrorKind::SuiMoveVerificationError,
            E::MovePrimitiveRuntimeError(location) => {
                message.error_details = location.0.map(|l| {
                    let mut abort = MoveAbort::default();
                    abort.location = Some(l.into());
                    ErrorDetails::Abort(abort)
                });
                ExecutionErrorKind::MovePrimitiveRuntimeError
            }
            E::MoveAbort(location, code) => {
                let mut abort = MoveAbort::default();
                abort.abort_code = Some(code);
                abort.location = Some(location.into());
                message.error_details = Some(ErrorDetails::Abort(abort));
                ExecutionErrorKind::MoveAbort
            }
            E::VMVerificationOrDeserializationError => {
                ExecutionErrorKind::VmVerificationOrDeserializationError
            }
            E::VMInvariantViolation => ExecutionErrorKind::VmInvariantViolation,
            E::FunctionNotFound => ExecutionErrorKind::FunctionNotFound,
            E::ArityMismatch => ExecutionErrorKind::ArityMismatch,
            E::TypeArityMismatch => ExecutionErrorKind::TypeArityMismatch,
            E::NonEntryFunctionInvoked => ExecutionErrorKind::NonEntryFunctionInvoked,
            E::CommandArgumentError { arg_idx, kind } => {
                let mut command_argument_error = CommandArgumentError::from(kind);
                command_argument_error.argument = Some(arg_idx.into());
                message.error_details =
                    Some(ErrorDetails::CommandArgumentError(command_argument_error));
                ExecutionErrorKind::CommandArgumentError
            }
            E::TypeArgumentError { argument_idx, kind } => {
                let mut type_argument_error = TypeArgumentError::default();
                type_argument_error.type_argument = Some(argument_idx.into());
                type_argument_error.kind =
                    Some(type_argument_error::TypeArgumentErrorKind::from(kind).into());
                message.error_details = Some(ErrorDetails::TypeArgumentError(type_argument_error));
                ExecutionErrorKind::TypeArgumentError
            }
            E::UnusedValueWithoutDrop {
                result_idx,
                secondary_idx,
            } => {
                message.error_details = Some(ErrorDetails::IndexError(index_error(
                    result_idx.into(),
                    Some(secondary_idx.into()),
                )));
                ExecutionErrorKind::UnusedValueWithoutDrop
            }
            E::InvalidPublicFunctionReturnType { idx } => {
                message.error_details =
                    Some(ErrorDetails::IndexError(index_error(idx.into(), None)));
                ExecutionErrorKind::InvalidPublicFunctionReturnType
            }
            E::InvalidTransferObject => ExecutionErrorKind::InvalidTransferObject,
            E::EffectsTooLarge {
                current_size,
                max_size,
            } => {
                message.error_details =
                    Some(ErrorDetails::SizeError(size_error(current_size, max_size)));
                ExecutionErrorKind::EffectsTooLarge
            }
            E::PublishUpgradeMissingDependency => {
                ExecutionErrorKind::PublishUpgradeMissingDependency
            }
            E::PublishUpgradeDependencyDowngrade => {
                ExecutionErrorKind::PublishUpgradeDependencyDowngrade
            }
            E::PackageUpgradeError { upgrade_error } => {
                message.error_details =
                    Some(ErrorDetails::PackageUpgradeError(upgrade_error.into()));
                ExecutionErrorKind::PackageUpgradeError
            }
            E::WrittenObjectsTooLarge {
                current_size,
                max_size,
            } => {
                message.error_details =
                    Some(ErrorDetails::SizeError(size_error(current_size, max_size)));

                ExecutionErrorKind::WrittenObjectsTooLarge
            }
            E::CertificateDenied => ExecutionErrorKind::CertificateDenied,
            E::SuiMoveVerificationTimedout => ExecutionErrorKind::SuiMoveVerificationTimedout,
            E::SharedObjectOperationNotAllowed => {
                ExecutionErrorKind::ConsensusObjectOperationNotAllowed
            }
            E::InputObjectDeleted => ExecutionErrorKind::InputObjectDeleted,
            E::ExecutionCancelledDueToSharedObjectCongestion { congested_objects } => {
                message.error_details = Some(ErrorDetails::CongestedObjects({
                    let mut message = CongestedObjects::default();
                    message.objects = congested_objects
                        .0
                        .iter()
                        .map(|o| o.to_canonical_string(true))
                        .collect();
                    message
                }));

                ExecutionErrorKind::ExecutionCanceledDueToConsensusObjectCongestion
            }
            E::AddressDeniedForCoin { address, coin_type } => {
                message.error_details = Some(ErrorDetails::CoinDenyListError({
                    let mut message = CoinDenyListError::default();
                    message.address = Some(address.to_string());
                    message.coin_type = Some(coin_type);
                    message
                }));
                ExecutionErrorKind::AddressDeniedForCoin
            }
            E::CoinTypeGlobalPause { coin_type } => {
                message.error_details = Some(ErrorDetails::CoinDenyListError({
                    let mut message = CoinDenyListError::default();
                    message.coin_type = Some(coin_type);
                    message
                }));
                ExecutionErrorKind::CoinTypeGlobalPause
            }
            E::ExecutionCancelledDueToRandomnessUnavailable => {
                ExecutionErrorKind::ExecutionCanceledDueToRandomnessUnavailable
            }
            E::MoveVectorElemTooBig {
                value_size,
                max_scaled_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(size_error(
                    value_size,
                    max_scaled_size,
                )));

                ExecutionErrorKind::MoveVectorElemTooBig
            }
            E::MoveRawValueTooBig {
                value_size,
                max_scaled_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(size_error(
                    value_size,
                    max_scaled_size,
                )));
                ExecutionErrorKind::MoveRawValueTooBig
            }
            E::InvalidLinkage => ExecutionErrorKind::InvalidLinkage,
            E::InsufficientBalanceForWithdraw => ExecutionErrorKind::InsufficientBalanceForWithdraw,
            E::NonExclusiveWriteInputObjectModified { id } => {
                message.set_object_id(id.to_canonical_string(true));
                ExecutionErrorKind::NonExclusiveWriteInputObjectModified
            }
        };

        message.set_kind(kind);
        message
    }
}

//
// CommandArgumentError
//

impl From<crate::execution_status::CommandArgumentError> for CommandArgumentError {
    fn from(value: crate::execution_status::CommandArgumentError) -> Self {
        use crate::execution_status::CommandArgumentError as E;
        use command_argument_error::CommandArgumentErrorKind;

        let mut message = Self::default();

        let kind = match value {
            E::TypeMismatch => CommandArgumentErrorKind::TypeMismatch,
            E::InvalidBCSBytes => CommandArgumentErrorKind::InvalidBcsBytes,
            E::InvalidUsageOfPureArg => CommandArgumentErrorKind::InvalidUsageOfPureArgument,
            E::InvalidArgumentToPrivateEntryFunction => {
                CommandArgumentErrorKind::InvalidArgumentToPrivateEntryFunction
            }
            E::IndexOutOfBounds { idx } => {
                message.index_error = Some(index_error(idx.into(), None));
                CommandArgumentErrorKind::IndexOutOfBounds
            }
            E::SecondaryIndexOutOfBounds {
                result_idx,
                secondary_idx,
            } => {
                message.index_error =
                    Some(index_error(result_idx.into(), Some(secondary_idx.into())));
                CommandArgumentErrorKind::SecondaryIndexOutOfBounds
            }
            E::InvalidResultArity { result_idx } => {
                message.index_error = Some(index_error(result_idx.into(), None));
                CommandArgumentErrorKind::InvalidResultArity
            }
            E::InvalidGasCoinUsage => CommandArgumentErrorKind::InvalidGasCoinUsage,
            E::InvalidValueUsage => CommandArgumentErrorKind::InvalidValueUsage,
            E::InvalidObjectByValue => CommandArgumentErrorKind::InvalidObjectByValue,
            E::InvalidObjectByMutRef => CommandArgumentErrorKind::InvalidObjectByMutRef,
            E::SharedObjectOperationNotAllowed => {
                CommandArgumentErrorKind::ConsensusObjectOperationNotAllowed
            }
            E::InvalidArgumentArity => CommandArgumentErrorKind::InvalidArgumentArity,

            E::InvalidTransferObject => CommandArgumentErrorKind::InvalidTransferObject,
            E::InvalidMakeMoveVecNonObjectArgument => {
                CommandArgumentErrorKind::InvalidMakeMoveVecNonObjectArgument
            }
            E::ArgumentWithoutValue => CommandArgumentErrorKind::ArgumentWithoutValue,
            E::CannotMoveBorrowedValue => CommandArgumentErrorKind::CannotMoveBorrowedValue,
            E::CannotWriteToExtendedReference => {
                CommandArgumentErrorKind::CannotWriteToExtendedReference
            }
            E::InvalidReferenceArgument => CommandArgumentErrorKind::InvalidReferenceArgument,
        };

        message.set_kind(kind);
        message
    }
}

//
// TypeArgumentError
//

impl From<crate::execution_status::TypeArgumentError>
    for type_argument_error::TypeArgumentErrorKind
{
    fn from(value: crate::execution_status::TypeArgumentError) -> Self {
        use crate::execution_status::TypeArgumentError::*;

        match value {
            TypeNotFound => Self::TypeNotFound,
            ConstraintNotSatisfied => Self::ConstraintNotSatisfied,
        }
    }
}

//
// PackageUpgradeError
//

impl From<crate::execution_status::PackageUpgradeError> for PackageUpgradeError {
    fn from(value: crate::execution_status::PackageUpgradeError) -> Self {
        use crate::execution_status::PackageUpgradeError as E;
        use package_upgrade_error::PackageUpgradeErrorKind;

        let mut message = Self::default();

        let kind = match value {
            E::UnableToFetchPackage { package_id } => {
                message.package_id = Some(package_id.to_canonical_string(true));
                PackageUpgradeErrorKind::UnableToFetchPackage
            }
            E::NotAPackage { object_id } => {
                message.package_id = Some(object_id.to_canonical_string(true));
                PackageUpgradeErrorKind::NotAPackage
            }
            E::IncompatibleUpgrade => PackageUpgradeErrorKind::IncompatibleUpgrade,
            E::DigestDoesNotMatch { digest } => {
                message.digest = crate::digests::Digest::try_from(digest)
                    .ok()
                    .map(|d| d.to_string());
                PackageUpgradeErrorKind::DigestDoesNotMatch
            }
            E::UnknownUpgradePolicy { policy } => {
                message.policy = Some(policy.into());
                PackageUpgradeErrorKind::UnknownUpgradePolicy
            }
            E::PackageIDDoesNotMatch {
                package_id,
                ticket_id,
            } => {
                message.package_id = Some(package_id.to_canonical_string(true));
                message.ticket_id = Some(ticket_id.to_canonical_string(true));
                PackageUpgradeErrorKind::PackageIdDoesNotMatch
            }
        };

        message.set_kind(kind);
        message
    }
}

//
// MoveLocation
//

impl From<crate::execution_status::MoveLocation> for MoveLocation {
    fn from(value: crate::execution_status::MoveLocation) -> Self {
        let mut message = Self::default();
        message.package = Some(value.module.address().to_canonical_string(true));
        message.module = Some(value.module.name().to_string());
        message.function = Some(value.function.into());
        message.instruction = Some(value.instruction.into());
        message.function_name = value.function_name.map(|name| name.to_string());
        message
    }
}

//
// AuthorityQuorumSignInfo aka ValidatorAggregatedSignature
//

impl<const T: bool> From<crate::crypto::AuthorityQuorumSignInfo<T>>
    for ValidatorAggregatedSignature
{
    fn from(value: crate::crypto::AuthorityQuorumSignInfo<T>) -> Self {
        let mut bitmap = Vec::new();
        value.signers_map.serialize_into(&mut bitmap).unwrap();

        Self::default()
            .with_epoch(value.epoch)
            .with_signature(value.signature.as_ref().to_vec())
            .with_bitmap(bitmap)
    }
}

impl<const T: bool> TryFrom<&ValidatorAggregatedSignature>
    for crate::crypto::AuthorityQuorumSignInfo<T>
{
    type Error = TryFromProtoError;

    fn try_from(value: &ValidatorAggregatedSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch: value.epoch(),
            signature: crate::crypto::AggregateAuthoritySignature::from_bytes(value.signature())
                .map_err(|e| TryFromProtoError::invalid("signature", e))?,
            signers_map: crate::sui_serde::deserialize_sui_bitmap(value.bitmap())
                .map_err(|e| TryFromProtoError::invalid("bitmap", e))?,
        })
    }
}

//
// ValidatorCommittee
//

impl From<crate::committee::Committee> for ValidatorCommittee {
    fn from(value: crate::committee::Committee) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.members = value
            .voting_rights
            .into_iter()
            .map(|(name, weight)| {
                let mut member = ValidatorCommitteeMember::default();
                member.public_key = Some(name.0.to_vec().into());
                member.weight = Some(weight);
                member
            })
            .collect();
        message
    }
}

//
// ZkLoginAuthenticator
//

impl From<&crate::zk_login_authenticator::ZkLoginAuthenticator> for ZkLoginAuthenticator {
    fn from(value: &crate::zk_login_authenticator::ZkLoginAuthenticator) -> Self {
        //TODO implement this without going through the sdk type
        let mut inputs = ZkLoginInputs::default();
        inputs.address_seed = Some(value.inputs.get_address_seed().to_string());
        let mut message = Self::default();
        message.inputs = Some(inputs);
        message.max_epoch = Some(value.get_max_epoch());
        message.signature = Some(value.user_signature.clone().into());

        sui_sdk_types::ZkLoginAuthenticator::try_from(value.clone())
            .map(Into::into)
            .ok()
            .unwrap_or(message)
    }
}

//
// ZkLoginPublicIdentifier
//

impl From<&crate::crypto::ZkLoginPublicIdentifier> for ZkLoginPublicIdentifier {
    fn from(value: &crate::crypto::ZkLoginPublicIdentifier) -> Self {
        //TODO implement this without going through the sdk type
        sui_sdk_types::ZkLoginPublicIdentifier::try_from(value.to_owned())
            .map(|id| (&id).into())
            .ok()
            .unwrap_or_default()
    }
}

//
// SignatureScheme
//

impl From<crate::crypto::SignatureScheme> for SignatureScheme {
    fn from(value: crate::crypto::SignatureScheme) -> Self {
        use crate::crypto::SignatureScheme as S;

        match value {
            S::ED25519 => Self::Ed25519,
            S::Secp256k1 => Self::Secp256k1,
            S::Secp256r1 => Self::Secp256r1,
            S::BLS12381 => Self::Bls12381,
            S::MultiSig => Self::Multisig,
            S::ZkLoginAuthenticator => Self::Zklogin,
            S::PasskeyAuthenticator => Self::Passkey,
        }
    }
}

//
// SimpleSignature
//

impl From<crate::crypto::Signature> for SimpleSignature {
    fn from(value: crate::crypto::Signature) -> Self {
        Self::from(&value)
    }
}

impl From<&crate::crypto::Signature> for SimpleSignature {
    fn from(value: &crate::crypto::Signature) -> Self {
        let scheme: SignatureScheme = value.scheme().into();
        let signature = value.signature_bytes();
        let public_key = value.public_key_bytes();

        let mut message = Self::default();
        message.scheme = Some(scheme.into());
        message.signature = Some(signature.to_vec().into());
        message.public_key = Some(public_key.to_vec().into());
        message
    }
}

//
// PasskeyAuthenticator
//

impl From<&crate::passkey_authenticator::PasskeyAuthenticator> for PasskeyAuthenticator {
    fn from(value: &crate::passkey_authenticator::PasskeyAuthenticator) -> Self {
        let mut message = Self::default();
        message.authenticator_data = Some(value.authenticator_data().to_vec().into());
        message.client_data_json = Some(value.client_data_json().to_owned());
        message.signature = Some(value.signature().into());
        message
    }
}

//
// MultisigMemberPublicKey
//

impl From<&crate::crypto::PublicKey> for MultisigMemberPublicKey {
    fn from(value: &crate::crypto::PublicKey) -> Self {
        let mut message = Self::default();

        match value {
            crate::crypto::PublicKey::Ed25519(_)
            | crate::crypto::PublicKey::Secp256k1(_)
            | crate::crypto::PublicKey::Secp256r1(_)
            | crate::crypto::PublicKey::Passkey(_) => {
                message.public_key = Some(value.as_ref().to_vec().into());
            }
            crate::crypto::PublicKey::ZkLogin(z) => {
                message.zklogin = Some(z.into());
            }
        }

        message.set_scheme(value.scheme().into());
        message
    }
}

//
// MultisigCommittee
//

impl From<&crate::multisig::MultiSigPublicKey> for MultisigCommittee {
    fn from(value: &crate::multisig::MultiSigPublicKey) -> Self {
        let mut message = Self::default();
        message.members = value
            .pubkeys()
            .iter()
            .map(|(pk, weight)| {
                let mut member = MultisigMember::default();
                member.public_key = Some(pk.into());
                member.weight = Some((*weight).into());
                member
            })
            .collect();
        message.threshold = Some((*value.threshold()).into());
        message
    }
}

impl From<&crate::multisig_legacy::MultiSigPublicKeyLegacy> for MultisigCommittee {
    fn from(value: &crate::multisig_legacy::MultiSigPublicKeyLegacy) -> Self {
        let mut message = Self::default();
        message.members = value
            .pubkeys()
            .iter()
            .map(|(pk, weight)| {
                let mut member = MultisigMember::default();
                member.public_key = Some(pk.into());
                member.weight = Some((*weight).into());
                member
            })
            .collect();
        message.threshold = Some((*value.threshold()).into());
        message
    }
}

//
// MultisigMemberSignature
//

impl From<&crate::crypto::CompressedSignature> for MultisigMemberSignature {
    fn from(value: &crate::crypto::CompressedSignature) -> Self {
        let mut message = Self::default();

        let scheme = match value {
            crate::crypto::CompressedSignature::Ed25519(b) => {
                message.signature = Some(b.0.to_vec().into());
                SignatureScheme::Ed25519
            }
            crate::crypto::CompressedSignature::Secp256k1(b) => {
                message.signature = Some(b.0.to_vec().into());
                SignatureScheme::Secp256k1
            }
            crate::crypto::CompressedSignature::Secp256r1(b) => {
                message.signature = Some(b.0.to_vec().into());
                SignatureScheme::Secp256r1
            }
            crate::crypto::CompressedSignature::ZkLogin(_z) => {
                //TODO
                SignatureScheme::Zklogin
            }
            crate::crypto::CompressedSignature::Passkey(_p) => {
                //TODO
                SignatureScheme::Passkey
            }
        };

        message.set_scheme(scheme);
        message
    }
}

//
// MultisigAggregatedSignature
//

impl From<&crate::multisig_legacy::MultiSigLegacy> for MultisigAggregatedSignature {
    fn from(value: &crate::multisig_legacy::MultiSigLegacy) -> Self {
        let mut legacy_bitmap = Vec::new();
        value
            .get_bitmap()
            .serialize_into(&mut legacy_bitmap)
            .unwrap();

        Self::default()
            .with_signatures(value.get_sigs().iter().map(Into::into).collect())
            .with_legacy_bitmap(legacy_bitmap)
            .with_committee(value.get_pk())
    }
}

impl From<&crate::multisig::MultiSig> for MultisigAggregatedSignature {
    fn from(value: &crate::multisig::MultiSig) -> Self {
        let mut message = Self::default();
        message.signatures = value.get_sigs().iter().map(Into::into).collect();
        message.bitmap = Some(value.get_bitmap().into());
        message.committee = Some(value.get_pk().into());
        message
    }
}

//
// UserSignature
//

impl From<crate::signature::GenericSignature> for UserSignature {
    fn from(value: crate::signature::GenericSignature) -> Self {
        Self::merge_from(&value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<&crate::signature::GenericSignature> for UserSignature {
    fn merge(&mut self, source: &crate::signature::GenericSignature, mask: &FieldMaskTree) {
        use user_signature::Signature;

        if mask.contains(Self::BCS_FIELD) {
            let mut bcs = Bcs::from(source.as_ref().to_vec());
            bcs.name = Some("UserSignatureBytes".to_owned());
            self.bcs = Some(bcs);
        }

        let scheme = match source {
            crate::signature::GenericSignature::MultiSig(multi_sig) => {
                if mask.contains(Self::MULTISIG_FIELD) {
                    self.signature = Some(Signature::Multisig(multi_sig.into()));
                }
                SignatureScheme::Multisig
            }
            crate::signature::GenericSignature::MultiSigLegacy(multi_sig_legacy) => {
                if mask.contains(Self::MULTISIG_FIELD) {
                    self.signature = Some(Signature::Multisig(multi_sig_legacy.into()));
                }
                SignatureScheme::Multisig
            }
            crate::signature::GenericSignature::Signature(signature) => {
                let scheme = signature.scheme().into();
                if mask.contains(Self::SIMPLE_FIELD) {
                    self.signature = Some(Signature::Simple(signature.into()));
                }
                scheme
            }
            crate::signature::GenericSignature::ZkLoginAuthenticator(z) => {
                if mask.contains(Self::ZKLOGIN_FIELD) {
                    self.signature = Some(Signature::Zklogin(z.into()));
                }
                SignatureScheme::Zklogin
            }
            crate::signature::GenericSignature::PasskeyAuthenticator(p) => {
                if mask.contains(Self::PASSKEY_FIELD) {
                    self.signature = Some(Signature::Passkey(p.into()));
                }
                SignatureScheme::Passkey
            }
        };

        if mask.contains(Self::SCHEME_FIELD) {
            self.set_scheme(scheme);
        }
    }
}

//
// BalanceChange
//

impl From<crate::balance_change::BalanceChange> for BalanceChange {
    fn from(value: crate::balance_change::BalanceChange) -> Self {
        let mut message = Self::default();
        message.address = Some(value.address.to_string());
        message.coin_type = Some(value.coin_type.to_canonical_string(true));
        message.amount = Some(value.amount.to_string());
        message
    }
}

impl TryFrom<&BalanceChange> for crate::balance_change::BalanceChange {
    type Error = TryFromProtoError;

    fn try_from(value: &BalanceChange) -> Result<Self, Self::Error> {
        Ok(Self {
            address: value
                .address()
                .parse()
                .map_err(|e| TryFromProtoError::invalid(BalanceChange::ADDRESS_FIELD, e))?,
            coin_type: value
                .coin_type()
                .parse()
                .map_err(|e| TryFromProtoError::invalid(BalanceChange::COIN_TYPE_FIELD, e))?,
            amount: value
                .amount()
                .parse()
                .map_err(|e| TryFromProtoError::invalid(BalanceChange::AMOUNT_FIELD, e))?,
        })
    }
}

//
// Object
//

pub const PACKAGE_TYPE: &str = "package";

impl From<crate::object::Object> for Object {
    fn from(value: crate::object::Object) -> Self {
        Self::merge_from(&value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<&crate::object::Object> for Object {
    fn merge(&mut self, source: &crate::object::Object, mask: &FieldMaskTree) {
        if mask.contains(Self::BCS_FIELD.name) {
            let mut bcs = Bcs::serialize(&source).unwrap();
            bcs.name = Some("Object".to_owned());
            self.bcs = Some(bcs);
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        if mask.contains(Self::OBJECT_ID_FIELD.name) {
            self.object_id = Some(source.id().to_canonical_string(true));
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(source.version().value());
        }

        if mask.contains(Self::OWNER_FIELD.name) {
            self.owner = Some(source.owner().to_owned().into());
        }

        if mask.contains(Self::PREVIOUS_TRANSACTION_FIELD.name) {
            self.previous_transaction = Some(source.previous_transaction.to_string());
        }

        if mask.contains(Self::STORAGE_REBATE_FIELD.name) {
            self.storage_rebate = Some(source.storage_rebate);
        }

        if mask.contains(Self::BALANCE_FIELD) {
            self.balance = source.as_coin_maybe().map(|coin| coin.balance.value());
        }

        self.merge(&source.data, mask);
    }
}

impl Merge<&crate::object::MoveObject> for Object {
    fn merge(&mut self, source: &crate::object::MoveObject, mask: &FieldMaskTree) {
        self.object_id = Some(source.id().to_canonical_string(true));
        self.version = Some(source.version().value());

        if mask.contains(Self::OBJECT_TYPE_FIELD.name) {
            self.object_type = Some(source.type_().to_canonical_string(true));
        }

        if mask.contains(Self::HAS_PUBLIC_TRANSFER_FIELD.name) {
            self.has_public_transfer = Some(source.has_public_transfer());
        }

        if mask.contains(Self::CONTENTS_FIELD.name) {
            let mut bcs = Bcs::from(source.contents().to_vec());
            bcs.name = Some(source.type_().to_canonical_string(true));
            self.contents = Some(bcs);
        }
    }
}

impl Merge<&crate::move_package::MovePackage> for Object {
    fn merge(&mut self, source: &crate::move_package::MovePackage, mask: &FieldMaskTree) {
        self.object_id = Some(source.id().to_canonical_string(true));
        self.version = Some(source.version().value());

        if mask.contains(Self::OBJECT_TYPE_FIELD.name) {
            self.object_type = Some(PACKAGE_TYPE.to_owned());
        }

        if mask.contains(Self::PACKAGE_FIELD.name) {
            let mut package = Package::default();
            package.modules = source
                .serialized_module_map()
                .iter()
                .map(|(name, contents)| {
                    let mut module = Module::default();
                    module.name = Some(name.to_string());
                    module.contents = Some(contents.clone().into());
                    module
                })
                .collect();
            package.type_origins = source
                .type_origin_table()
                .clone()
                .into_iter()
                .map(Into::into)
                .collect();
            package.linkage = source
                .linkage_table()
                .iter()
                .map(
                    |(
                        original_id,
                        crate::move_package::UpgradeInfo {
                            upgraded_id,
                            upgraded_version,
                        },
                    )| {
                        let mut linkage = Linkage::default();
                        linkage.original_id = Some(original_id.to_canonical_string(true));
                        linkage.upgraded_id = Some(upgraded_id.to_canonical_string(true));
                        linkage.upgraded_version = Some(upgraded_version.value());
                        linkage
                    },
                )
                .collect();

            self.package = Some(package);
        }
    }
}

impl Merge<&crate::object::Data> for Object {
    fn merge(&mut self, source: &crate::object::Data, mask: &FieldMaskTree) {
        match source {
            crate::object::Data::Move(object) => self.merge(object, mask),
            crate::object::Data::Package(package) => self.merge(package, mask),
        }
    }
}

//
// TypeOrigin
//

impl From<crate::move_package::TypeOrigin> for TypeOrigin {
    fn from(value: crate::move_package::TypeOrigin) -> Self {
        let mut message = Self::default();
        message.module_name = Some(value.module_name.to_string());
        message.datatype_name = Some(value.datatype_name.to_string());
        message.package_id = Some(value.package.to_canonical_string(true));
        message
    }
}

//
// GenesisObject
//

impl From<crate::transaction::GenesisObject> for Object {
    fn from(value: crate::transaction::GenesisObject) -> Self {
        let crate::transaction::GenesisObject::RawObject { data, owner } = value;
        let mut message = Self::default();
        message.owner = Some(owner.into());

        message.merge(&data, &FieldMaskTree::new_wildcard());

        message
    }
}

//
// ObjectReference
//

pub trait ObjectRefExt {
    fn to_proto(self) -> ObjectReference;
}

pub trait ObjectReferenceExt {
    fn try_to_object_ref(&self) -> Result<crate::base_types::ObjectRef, anyhow::Error>;
}

impl ObjectRefExt for crate::base_types::ObjectRef {
    fn to_proto(self) -> ObjectReference {
        let (object_id, version, digest) = self;
        let mut message = ObjectReference::default();
        message.object_id = Some(object_id.to_canonical_string(true));
        message.version = Some(version.value());
        message.digest = Some(digest.to_string());
        message
    }
}

impl ObjectReferenceExt for ObjectReference {
    fn try_to_object_ref(&self) -> Result<crate::base_types::ObjectRef, anyhow::Error> {
        use anyhow::Context;

        let object_id = self
            .object_id_opt()
            .ok_or_else(|| anyhow::anyhow!("missing object_id"))?;
        let object_id = crate::base_types::ObjectID::from_hex_literal(object_id)
            .with_context(|| format!("Failed to parse object_id: {}", object_id))?;

        let version = self
            .version_opt()
            .ok_or_else(|| anyhow::anyhow!("missing version"))?;
        let version = crate::base_types::SequenceNumber::from(version);

        let digest = self
            .digest_opt()
            .ok_or_else(|| anyhow::anyhow!("missing digest"))?;
        let digest = digest
            .parse::<crate::digests::ObjectDigest>()
            .with_context(|| format!("Failed to parse digest: {}", digest))?;

        Ok((object_id, version, digest))
    }
}

impl From<&crate::storage::ObjectKey> for ObjectReference {
    fn from(value: &crate::storage::ObjectKey) -> Self {
        Self::default()
            .with_object_id(value.0.to_canonical_string(true))
            .with_version(value.1.value())
    }
}

//
// Owner
//

impl From<crate::object::Owner> for Owner {
    fn from(value: crate::object::Owner) -> Self {
        use crate::object::Owner as O;
        use owner::OwnerKind;

        let mut message = Self::default();

        let kind = match value {
            O::AddressOwner(address) => {
                message.address = Some(address.to_string());
                OwnerKind::Address
            }
            O::ObjectOwner(address) => {
                message.address = Some(address.to_string());
                OwnerKind::Object
            }
            O::Shared {
                initial_shared_version,
            } => {
                message.version = Some(initial_shared_version.value());
                OwnerKind::Shared
            }
            O::Immutable => OwnerKind::Immutable,
            O::ConsensusAddressOwner {
                start_version,
                owner,
            } => {
                message.version = Some(start_version.value());
                message.address = Some(owner.to_string());
                OwnerKind::ConsensusAddress
            }
        };

        message.set_kind(kind);
        message
    }
}

//
// Transaction
//

impl From<crate::transaction::TransactionData> for Transaction {
    fn from(value: crate::transaction::TransactionData) -> Self {
        Self::merge_from(&value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<&crate::transaction::TransactionData> for Transaction {
    fn merge(&mut self, source: &crate::transaction::TransactionData, mask: &FieldMaskTree) {
        if mask.contains(Self::BCS_FIELD.name) {
            let mut bcs = Bcs::serialize(&source).unwrap();
            bcs.name = Some("TransactionData".to_owned());
            self.bcs = Some(bcs);
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(1);
        }

        let crate::transaction::TransactionData::V1(source) = source;

        if mask.contains(Self::KIND_FIELD.name) {
            self.kind = Some(source.kind.clone().into());
        }

        if mask.contains(Self::SENDER_FIELD.name) {
            self.sender = Some(source.sender.to_string());
        }

        if mask.contains(Self::GAS_PAYMENT_FIELD.name) {
            self.gas_payment = Some((&source.gas_data).into());
        }

        if mask.contains(Self::EXPIRATION_FIELD.name) {
            self.expiration = Some(source.expiration.into());
        }
    }
}

//
// GasPayment
//

impl From<&crate::transaction::GasData> for GasPayment {
    fn from(value: &crate::transaction::GasData) -> Self {
        let mut message = Self::default();
        message.objects = value
            .payment
            .iter()
            .map(|obj_ref| obj_ref.to_proto())
            .collect();
        message.owner = Some(value.owner.to_string());
        message.price = Some(value.price);
        message.budget = Some(value.budget);
        message
    }
}

//
// TransactionExpiration
//

impl From<crate::transaction::TransactionExpiration> for TransactionExpiration {
    fn from(value: crate::transaction::TransactionExpiration) -> Self {
        use crate::transaction::TransactionExpiration as E;
        use transaction_expiration::TransactionExpirationKind;

        let mut message = Self::default();

        let kind = match value {
            E::None => TransactionExpirationKind::None,
            E::Epoch(epoch) => {
                message.epoch = Some(epoch);
                TransactionExpirationKind::Epoch
            }
            E::ValidDuring {
                min_epoch,
                max_epoch,
                min_timestamp_seconds,
                max_timestamp_seconds,
                chain,
                nonce,
            } => {
                message.epoch = max_epoch;
                message.min_epoch = min_epoch;
                message.min_timestamp =
                    min_timestamp_seconds.map(|seconds| prost_types::Timestamp {
                        seconds: seconds as _,
                        nanos: 0,
                    });
                message.max_timestamp =
                    max_timestamp_seconds.map(|seconds| prost_types::Timestamp {
                        seconds: seconds as _,
                        nanos: 0,
                    });
                message.set_chain(sui_sdk_types::Digest::new(*chain.as_bytes()));
                message.set_nonce(nonce);

                TransactionExpirationKind::ValidDuring
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&TransactionExpiration> for crate::transaction::TransactionExpiration {
    type Error = &'static str;

    fn try_from(value: &TransactionExpiration) -> Result<Self, Self::Error> {
        use transaction_expiration::TransactionExpirationKind;

        Ok(match value.kind() {
            TransactionExpirationKind::None => Self::None,
            TransactionExpirationKind::Epoch => Self::Epoch(value.epoch()),
            TransactionExpirationKind::Unknown | _ => {
                return Err("unknown TransactionExpirationKind");
            }
        })
    }
}

//
// TransactionKind
//

impl From<crate::transaction::TransactionKind> for TransactionKind {
    fn from(value: crate::transaction::TransactionKind) -> Self {
        use crate::transaction::TransactionKind as K;
        use transaction_kind::Kind;

        let message = Self::default();

        match value {
            K::ProgrammableTransaction(ptb) => message
                .with_programmable_transaction(ptb)
                .with_kind(Kind::ProgrammableTransaction),
            K::ChangeEpoch(change_epoch) => message
                .with_change_epoch(change_epoch)
                .with_kind(Kind::ChangeEpoch),
            K::Genesis(genesis) => message.with_genesis(genesis).with_kind(Kind::Genesis),
            K::ConsensusCommitPrologue(prologue) => message
                .with_consensus_commit_prologue(prologue)
                .with_kind(Kind::ConsensusCommitPrologueV1),
            K::AuthenticatorStateUpdate(update) => message
                .with_authenticator_state_update(update)
                .with_kind(Kind::AuthenticatorStateUpdate),
            K::EndOfEpochTransaction(transactions) => message
                .with_end_of_epoch({
                    EndOfEpochTransaction::default()
                        .with_transactions(transactions.into_iter().map(Into::into).collect())
                })
                .with_kind(Kind::EndOfEpoch),
            K::RandomnessStateUpdate(update) => message
                .with_randomness_state_update(update)
                .with_kind(Kind::RandomnessStateUpdate),
            K::ConsensusCommitPrologueV2(prologue) => message
                .with_consensus_commit_prologue(prologue)
                .with_kind(Kind::ConsensusCommitPrologueV2),
            K::ConsensusCommitPrologueV3(prologue) => message
                .with_consensus_commit_prologue(prologue)
                .with_kind(Kind::ConsensusCommitPrologueV3),
            K::ConsensusCommitPrologueV4(prologue) => message
                .with_consensus_commit_prologue(prologue)
                .with_kind(Kind::ConsensusCommitPrologueV4),
            K::ProgrammableSystemTransaction(_) => message,
            // TODO support ProgrammableSystemTransaction
            // .with_programmable_transaction(ptb)
            // .with_kind(Kind::ProgrammableSystemTransaction),
        }
    }
}

//
// ConsensusCommitPrologue
//

impl From<crate::messages_consensus::ConsensusCommitPrologue> for ConsensusCommitPrologue {
    fn from(value: crate::messages_consensus::ConsensusCommitPrologue) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.round = Some(value.round);
        message.commit_timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(
            value.commit_timestamp_ms,
        ));
        message
    }
}

impl From<crate::messages_consensus::ConsensusCommitPrologueV2> for ConsensusCommitPrologue {
    fn from(value: crate::messages_consensus::ConsensusCommitPrologueV2) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.round = Some(value.round);
        message.commit_timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(
            value.commit_timestamp_ms,
        ));
        message.consensus_commit_digest = Some(value.consensus_commit_digest.to_string());
        message
    }
}

impl From<crate::messages_consensus::ConsensusCommitPrologueV3> for ConsensusCommitPrologue {
    fn from(value: crate::messages_consensus::ConsensusCommitPrologueV3) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.round = Some(value.round);
        message.commit_timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(
            value.commit_timestamp_ms,
        ));
        message.consensus_commit_digest = Some(value.consensus_commit_digest.to_string());
        message.sub_dag_index = value.sub_dag_index;
        message.consensus_determined_version_assignments =
            Some(value.consensus_determined_version_assignments.into());
        message
    }
}

impl From<crate::messages_consensus::ConsensusCommitPrologueV4> for ConsensusCommitPrologue {
    fn from(
        crate::messages_consensus::ConsensusCommitPrologueV4 {
            epoch,
            round,
            sub_dag_index,
            commit_timestamp_ms,
            consensus_commit_digest,
            consensus_determined_version_assignments,
            additional_state_digest,
        }: crate::messages_consensus::ConsensusCommitPrologueV4,
    ) -> Self {
        let mut message = Self::default();
        message.epoch = Some(epoch);
        message.round = Some(round);
        message.commit_timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(commit_timestamp_ms));
        message.consensus_commit_digest = Some(consensus_commit_digest.to_string());
        message.sub_dag_index = sub_dag_index;
        message.consensus_determined_version_assignments =
            Some(consensus_determined_version_assignments.into());
        message.additional_state_digest = Some(additional_state_digest.to_string());
        message
    }
}

//
// ConsensusDeterminedVersionAssignments
//

impl From<crate::messages_consensus::ConsensusDeterminedVersionAssignments>
    for ConsensusDeterminedVersionAssignments
{
    fn from(value: crate::messages_consensus::ConsensusDeterminedVersionAssignments) -> Self {
        use crate::messages_consensus::ConsensusDeterminedVersionAssignments as A;

        let mut message = Self::default();

        let version = match value {
            A::CancelledTransactions(canceled_transactions) => {
                message.canceled_transactions = canceled_transactions
                    .into_iter()
                    .map(|(tx_digest, assignments)| {
                        let mut message = CanceledTransaction::default();
                        message.digest = Some(tx_digest.to_string());
                        message.version_assignments = assignments
                            .into_iter()
                            .map(|(id, version)| {
                                let mut message = VersionAssignment::default();
                                message.object_id = Some(id.to_canonical_string(true));
                                message.version = Some(version.value());
                                message
                            })
                            .collect();
                        message
                    })
                    .collect();
                1
            }
            A::CancelledTransactionsV2(canceled_transactions) => {
                message.canceled_transactions = canceled_transactions
                    .into_iter()
                    .map(|(tx_digest, assignments)| {
                        let mut message = CanceledTransaction::default();
                        message.digest = Some(tx_digest.to_string());
                        message.version_assignments = assignments
                            .into_iter()
                            .map(|((id, start_version), version)| {
                                let mut message = VersionAssignment::default();
                                message.object_id = Some(id.to_canonical_string(true));
                                message.start_version = Some(start_version.value());
                                message.version = Some(version.value());
                                message
                            })
                            .collect();
                        message
                    })
                    .collect();
                2
            }
        };

        message.version = Some(version);
        message
    }
}

//
// GenesisTransaction
//

impl From<crate::transaction::GenesisTransaction> for GenesisTransaction {
    fn from(value: crate::transaction::GenesisTransaction) -> Self {
        let mut message = Self::default();
        message.objects = value.objects.into_iter().map(Into::into).collect();
        message
    }
}

//
// RandomnessStateUpdate
//

impl From<crate::transaction::RandomnessStateUpdate> for RandomnessStateUpdate {
    fn from(value: crate::transaction::RandomnessStateUpdate) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.randomness_round = Some(value.randomness_round.0);
        message.random_bytes = Some(value.random_bytes.into());
        message.randomness_object_initial_shared_version =
            Some(value.randomness_obj_initial_shared_version.value());
        message
    }
}

//
// AuthenticatorStateUpdate
//

impl From<crate::transaction::AuthenticatorStateUpdate> for AuthenticatorStateUpdate {
    fn from(value: crate::transaction::AuthenticatorStateUpdate) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.round = Some(value.round);
        message.new_active_jwks = value.new_active_jwks.into_iter().map(Into::into).collect();
        message.authenticator_object_initial_shared_version =
            Some(value.authenticator_obj_initial_shared_version.value());
        message
    }
}

//
// ActiveJwk
//

impl From<crate::authenticator_state::ActiveJwk> for ActiveJwk {
    fn from(value: crate::authenticator_state::ActiveJwk) -> Self {
        let mut jwk_id = JwkId::default();
        jwk_id.iss = Some(value.jwk_id.iss);
        jwk_id.kid = Some(value.jwk_id.kid);

        let mut jwk = Jwk::default();
        jwk.kty = Some(value.jwk.kty);
        jwk.e = Some(value.jwk.e);
        jwk.n = Some(value.jwk.n);
        jwk.alg = Some(value.jwk.alg);

        let mut message = Self::default();
        message.id = Some(jwk_id);
        message.jwk = Some(jwk);
        message.epoch = Some(value.epoch);
        message
    }
}

//
// ChangeEpoch
//

impl From<crate::transaction::ChangeEpoch> for ChangeEpoch {
    fn from(value: crate::transaction::ChangeEpoch) -> Self {
        let mut message = Self::default();
        message.epoch = Some(value.epoch);
        message.protocol_version = Some(value.protocol_version.as_u64());
        message.storage_charge = Some(value.storage_charge);
        message.computation_charge = Some(value.computation_charge);
        message.storage_rebate = Some(value.storage_rebate);
        message.non_refundable_storage_fee = Some(value.non_refundable_storage_fee);
        message.epoch_start_timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(
            value.epoch_start_timestamp_ms,
        ));
        message.system_packages = value
            .system_packages
            .into_iter()
            .map(|(version, modules, dependencies)| {
                let mut message = SystemPackage::default();
                message.version = Some(version.value());
                message.modules = modules.into_iter().map(Into::into).collect();
                message.dependencies = dependencies
                    .iter()
                    .map(|d| d.to_canonical_string(true))
                    .collect();
                message
            })
            .collect();
        message
    }
}

//
// EndOfEpochTransactionkind
//

impl From<crate::transaction::EndOfEpochTransactionKind> for EndOfEpochTransactionKind {
    fn from(value: crate::transaction::EndOfEpochTransactionKind) -> Self {
        use crate::transaction::EndOfEpochTransactionKind as K;
        use end_of_epoch_transaction_kind::Kind;

        let message = Self::default();

        match value {
            K::ChangeEpoch(change_epoch) => message
                .with_change_epoch(change_epoch)
                .with_kind(Kind::ChangeEpoch),
            K::AuthenticatorStateCreate => message.with_kind(Kind::AuthenticatorStateCreate),
            K::AuthenticatorStateExpire(expire) => message
                .with_authenticator_state_expire(expire)
                .with_kind(Kind::AuthenticatorStateExpire),
            K::RandomnessStateCreate => message.with_kind(Kind::RandomnessStateCreate),
            K::DenyListStateCreate => message.with_kind(Kind::DenyListStateCreate),
            K::BridgeStateCreate(chain_id) => message
                .with_bridge_chain_id(chain_id.to_string())
                .with_kind(Kind::BridgeStateCreate),
            K::BridgeCommitteeInit(bridge_object_version) => message
                .with_bridge_object_version(bridge_object_version.into())
                .with_kind(Kind::BridgeCommitteeInit),
            K::StoreExecutionTimeObservations(observations) => message
                .with_execution_time_observations(observations)
                .with_kind(Kind::StoreExecutionTimeObservations),
            K::AccumulatorRootCreate => message.with_kind(Kind::AccumulatorRootCreate),
            K::CoinRegistryCreate => message.with_kind(Kind::CoinRegistryCreate),
            K::DisplayRegistryCreate => message.with_kind(Kind::DisplayRegistryCreate),
            K::AddressAliasStateCreate => {
                todo!("AddressAliasStateCreate needs to be added to proto in sui-apis")
            }
        }
    }
}

//
// AuthenticatorStateExpire
//

impl From<crate::transaction::AuthenticatorStateExpire> for AuthenticatorStateExpire {
    fn from(value: crate::transaction::AuthenticatorStateExpire) -> Self {
        let mut message = Self::default();
        message.min_epoch = Some(value.min_epoch);
        message.authenticator_object_initial_shared_version =
            Some(value.authenticator_obj_initial_shared_version.value());
        message
    }
}

// ExecutionTimeObservations

impl From<crate::transaction::StoredExecutionTimeObservations> for ExecutionTimeObservations {
    fn from(value: crate::transaction::StoredExecutionTimeObservations) -> Self {
        let mut message = Self::default();
        match value {
            crate::transaction::StoredExecutionTimeObservations::V1(vec) => {
                message.version = Some(1);
                message.observations = vec
                    .into_iter()
                    .map(|(key, observation)| {
                        use crate::execution::ExecutionTimeObservationKey as K;
                        use execution_time_observation::ExecutionTimeObservationKind;

                        let mut message = ExecutionTimeObservation::default();

                        let kind = match key {
                            K::MoveEntryPoint {
                                package,
                                module,
                                function,
                                type_arguments,
                            } => {
                                message.move_entry_point = Some({
                                    let mut message = MoveCall::default();
                                    message.package = Some(package.to_canonical_string(true));
                                    message.module = Some(module);
                                    message.function = Some(function);
                                    message.type_arguments = type_arguments
                                        .into_iter()
                                        .map(|ty| ty.to_canonical_string(true))
                                        .collect();
                                    message
                                });
                                ExecutionTimeObservationKind::MoveEntryPoint
                            }
                            K::TransferObjects => ExecutionTimeObservationKind::TransferObjects,
                            K::SplitCoins => ExecutionTimeObservationKind::SplitCoins,
                            K::MergeCoins => ExecutionTimeObservationKind::MergeCoins,
                            K::Publish => ExecutionTimeObservationKind::Publish,
                            K::MakeMoveVec => ExecutionTimeObservationKind::MakeMoveVector,
                            K::Upgrade => ExecutionTimeObservationKind::Upgrade,
                        };

                        message.validator_observations = observation
                            .into_iter()
                            .map(|(name, duration)| {
                                let mut message = ValidatorExecutionTimeObservation::default();
                                message.validator = Some(name.0.to_vec().into());
                                message.duration = Some(prost_types::Duration {
                                    seconds: duration.as_secs() as i64,
                                    nanos: duration.subsec_nanos() as i32,
                                });
                                message
                            })
                            .collect();

                        message.set_kind(kind);
                        message
                    })
                    .collect();
            }
        }

        message
    }
}

//
// ProgrammableTransaction
//

impl From<crate::transaction::ProgrammableTransaction> for ProgrammableTransaction {
    fn from(value: crate::transaction::ProgrammableTransaction) -> Self {
        let mut message = Self::default();
        message.inputs = value.inputs.into_iter().map(Into::into).collect();
        message.commands = value.commands.into_iter().map(Into::into).collect();
        message
    }
}

//
// Input
//

impl From<crate::transaction::CallArg> for Input {
    fn from(value: crate::transaction::CallArg) -> Self {
        use crate::transaction::CallArg as I;
        use crate::transaction::ObjectArg as O;
        use input::InputKind;
        use input::Mutability;

        let mut message = Self::default();

        let kind = match value {
            I::Pure(value) => {
                message.pure = Some(value.into());
                InputKind::Pure
            }
            I::Object(o) => match o {
                O::ImmOrOwnedObject((id, version, digest)) => {
                    message.object_id = Some(id.to_canonical_string(true));
                    message.version = Some(version.value());
                    message.digest = Some(digest.to_string());
                    InputKind::ImmutableOrOwned
                }
                O::SharedObject {
                    id,
                    initial_shared_version,
                    mutability,
                } => {
                    message.object_id = Some(id.to_canonical_string(true));
                    message.version = Some(initial_shared_version.value());
                    message.mutable = Some(mutability.is_exclusive());
                    message.set_mutability(match mutability {
                        crate::transaction::SharedObjectMutability::Immutable => {
                            Mutability::Immutable
                        }
                        crate::transaction::SharedObjectMutability::Mutable => Mutability::Mutable,
                        crate::transaction::SharedObjectMutability::NonExclusiveWrite => {
                            Mutability::NonExclusiveWrite
                        }
                    });
                    InputKind::Shared
                }
                O::Receiving((id, version, digest)) => {
                    message.object_id = Some(id.to_canonical_string(true));
                    message.version = Some(version.value());
                    message.digest = Some(digest.to_string());
                    InputKind::Receiving
                }
            },
            I::FundsWithdrawal(withdrawal) => {
                message.set_funds_withdrawal(withdrawal);
                InputKind::FundsWithdrawal
            }
        };

        message.set_kind(kind);
        message
    }
}

impl From<crate::transaction::FundsWithdrawalArg> for FundsWithdrawal {
    fn from(value: crate::transaction::FundsWithdrawalArg) -> Self {
        use funds_withdrawal::Source;

        let mut message = Self::default();

        message.amount = match value.reservation {
            crate::transaction::Reservation::EntireBalance => None,
            crate::transaction::Reservation::MaxAmountU64(amount) => Some(amount),
        };
        let crate::transaction::WithdrawalTypeArg::Balance(coin_type) = value.type_arg;
        message.coin_type = Some(coin_type.to_canonical_string(true));
        message.set_source(match value.withdraw_from {
            crate::transaction::WithdrawFrom::Sender => Source::Sender,
            crate::transaction::WithdrawFrom::Sponsor => Source::Sponsor,
        });

        message
    }
}

//
// Argument
//

impl From<crate::transaction::Argument> for Argument {
    fn from(value: crate::transaction::Argument) -> Self {
        use crate::transaction::Argument as A;
        use argument::ArgumentKind;

        let mut message = Self::default();

        let kind = match value {
            A::GasCoin => ArgumentKind::Gas,
            A::Input(input) => {
                message.input = Some(input.into());
                ArgumentKind::Input
            }
            A::Result(result) => {
                message.result = Some(result.into());
                ArgumentKind::Result
            }
            A::NestedResult(result, subresult) => {
                message.result = Some(result.into());
                message.subresult = Some(subresult.into());
                ArgumentKind::Result
            }
        };

        message.set_kind(kind);
        message
    }
}

//
// Command
//

impl From<crate::transaction::Command> for Command {
    fn from(value: crate::transaction::Command) -> Self {
        use crate::transaction::Command as C;
        use command::Command;

        let command = match value {
            C::MoveCall(move_call) => Command::MoveCall((*move_call).into()),
            C::TransferObjects(objects, address) => Command::TransferObjects({
                let mut message = TransferObjects::default();
                message.objects = objects.into_iter().map(Into::into).collect();
                message.address = Some(address.into());
                message
            }),
            C::SplitCoins(coin, amounts) => Command::SplitCoins({
                let mut message = SplitCoins::default();
                message.coin = Some(coin.into());
                message.amounts = amounts.into_iter().map(Into::into).collect();
                message
            }),
            C::MergeCoins(coin, coins_to_merge) => Command::MergeCoins({
                let mut message = MergeCoins::default();
                message.coin = Some(coin.into());
                message.coins_to_merge = coins_to_merge.into_iter().map(Into::into).collect();
                message
            }),
            C::Publish(modules, dependencies) => Command::Publish({
                let mut message = Publish::default();
                message.modules = modules.into_iter().map(Into::into).collect();
                message.dependencies = dependencies
                    .iter()
                    .map(|d| d.to_canonical_string(true))
                    .collect();
                message
            }),
            C::MakeMoveVec(element_type, elements) => Command::MakeMoveVector({
                let mut message = MakeMoveVector::default();
                message.element_type = element_type.map(|t| t.to_canonical_string(true));
                message.elements = elements.into_iter().map(Into::into).collect();
                message
            }),
            C::Upgrade(modules, dependencies, package, ticket) => Command::Upgrade({
                let mut message = Upgrade::default();
                message.modules = modules.into_iter().map(Into::into).collect();
                message.dependencies = dependencies
                    .iter()
                    .map(|d| d.to_canonical_string(true))
                    .collect();
                message.package = Some(package.to_canonical_string(true));
                message.ticket = Some(ticket.into());
                message
            }),
        };

        let mut message = Self::default();
        message.command = Some(command);
        message
    }
}

//
// MoveCall
//

impl From<crate::transaction::ProgrammableMoveCall> for MoveCall {
    fn from(value: crate::transaction::ProgrammableMoveCall) -> Self {
        let mut message = Self::default();
        message.package = Some(value.package.to_canonical_string(true));
        message.module = Some(value.module.to_string());
        message.function = Some(value.function.to_string());
        message.type_arguments = value
            .type_arguments
            .iter()
            .map(|t| t.to_canonical_string(true))
            .collect();
        message.arguments = value.arguments.into_iter().map(Into::into).collect();
        message
    }
}

//
// TransactionEffects
//

impl From<crate::effects::TransactionEffects> for TransactionEffects {
    fn from(value: crate::effects::TransactionEffects) -> Self {
        Self::merge_from(&value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<&crate::effects::TransactionEffects> for TransactionEffects {
    fn merge(&mut self, source: &crate::effects::TransactionEffects, mask: &FieldMaskTree) {
        if mask.contains(Self::BCS_FIELD.name) {
            let mut bcs = Bcs::serialize(&source).unwrap();
            bcs.name = Some("TransactionEffects".to_owned());
            self.bcs = Some(bcs);
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        match source {
            crate::effects::TransactionEffects::V1(v1) => self.merge(v1, mask),
            crate::effects::TransactionEffects::V2(v2) => self.merge(v2, mask),
        }
    }
}

//
// TransactionEffectsV1
//

impl Merge<&crate::effects::TransactionEffectsV1> for TransactionEffects {
    fn merge(&mut self, value: &crate::effects::TransactionEffectsV1, mask: &FieldMaskTree) {
        use crate::effects::TransactionEffectsAPI;

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(1);
        }

        if mask.contains(Self::STATUS_FIELD.name) {
            self.status = Some(value.status().clone().into());
        }

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = Some(value.executed_epoch());
        }

        if mask.contains(Self::GAS_USED_FIELD.name) {
            self.gas_used = Some(value.gas_cost_summary().clone().into());
        }

        if mask.contains(Self::TRANSACTION_DIGEST_FIELD.name) {
            self.transaction_digest = Some(value.transaction_digest().to_string());
        }

        if mask.contains(Self::EVENTS_DIGEST_FIELD.name) {
            self.events_digest = value.events_digest().map(|d| d.to_string());
        }

        if mask.contains(Self::DEPENDENCIES_FIELD.name) {
            self.dependencies = value
                .dependencies()
                .iter()
                .map(ToString::to_string)
                .collect();
        }

        if mask.contains(Self::CHANGED_OBJECTS_FIELD.name)
            || mask.contains(Self::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name)
            || mask.contains(Self::GAS_OBJECT_FIELD.name)
        {
            let mut changed_objects = Vec::new();
            let mut unchanged_consensus_objects = Vec::new();

            for ((id, version, digest), owner) in value.created() {
                let mut change = ChangedObject::default();
                change.object_id = Some(id.to_canonical_string(true));
                change.input_state = Some(changed_object::InputObjectState::DoesNotExist.into());
                change.output_state = Some(changed_object::OutputObjectState::ObjectWrite.into());
                change.output_version = Some(version.value());
                change.output_digest = Some(digest.to_string());
                change.output_owner = Some(owner.clone().into());
                change.id_operation = Some(changed_object::IdOperation::Created.into());

                changed_objects.push(change);
            }

            for ((id, version, digest), owner) in value.mutated() {
                let mut change = ChangedObject::default();
                change.object_id = Some(id.to_canonical_string(true));
                change.input_state = Some(changed_object::InputObjectState::Exists.into());
                change.output_state = Some(changed_object::OutputObjectState::ObjectWrite.into());
                change.output_version = Some(version.value());
                change.output_digest = Some(digest.to_string());
                change.output_owner = Some(owner.clone().into());
                change.id_operation = Some(changed_object::IdOperation::None.into());

                changed_objects.push(change);
            }

            for ((id, version, digest), owner) in value.unwrapped() {
                let mut change = ChangedObject::default();
                change.object_id = Some(id.to_canonical_string(true));
                change.input_state = Some(changed_object::InputObjectState::DoesNotExist.into());
                change.output_state = Some(changed_object::OutputObjectState::ObjectWrite.into());
                change.output_version = Some(version.value());
                change.output_digest = Some(digest.to_string());
                change.output_owner = Some(owner.clone().into());
                change.id_operation = Some(changed_object::IdOperation::None.into());

                changed_objects.push(change);
            }

            for (id, version, digest) in value.deleted() {
                let mut change = ChangedObject::default();
                change.object_id = Some(id.to_canonical_string(true));
                change.input_state = Some(changed_object::InputObjectState::Exists.into());
                change.output_state = Some(changed_object::OutputObjectState::DoesNotExist.into());
                change.output_version = Some(version.value());
                change.output_digest = Some(digest.to_string());
                change.id_operation = Some(changed_object::IdOperation::Deleted.into());

                changed_objects.push(change);
            }

            for (id, version, digest) in value.unwrapped_then_deleted() {
                let mut change = ChangedObject::default();
                change.object_id = Some(id.to_canonical_string(true));
                change.input_state = Some(changed_object::InputObjectState::DoesNotExist.into());
                change.output_state = Some(changed_object::OutputObjectState::DoesNotExist.into());
                change.output_version = Some(version.value());
                change.output_digest = Some(digest.to_string());
                change.id_operation = Some(changed_object::IdOperation::Deleted.into());

                changed_objects.push(change);
            }

            for (id, version, digest) in value.wrapped() {
                let mut change = ChangedObject::default();
                change.object_id = Some(id.to_canonical_string(true));
                change.input_state = Some(changed_object::InputObjectState::Exists.into());
                change.output_state = Some(changed_object::OutputObjectState::DoesNotExist.into());
                change.output_version = Some(version.value());
                change.output_digest = Some(digest.to_string());
                change.id_operation = Some(changed_object::IdOperation::Deleted.into());

                changed_objects.push(change);
            }

            for (object_id, version) in value.modified_at_versions() {
                let object_id = object_id.to_canonical_string(true);
                let version = version.value();
                if let Some(changed_object) = changed_objects
                    .iter_mut()
                    .find(|object| object.object_id() == object_id)
                {
                    changed_object.input_version = Some(version);
                }
            }

            for (id, version, digest) in value.shared_objects() {
                let object_id = id.to_canonical_string(true);
                let version = version.value();
                let digest = digest.to_string();

                if let Some(changed_object) = changed_objects
                    .iter_mut()
                    .find(|object| object.object_id() == object_id)
                {
                    changed_object.input_version = Some(version);
                    changed_object.input_digest = Some(digest);
                } else {
                    let mut unchanged_consensus_object = UnchangedConsensusObject::default();
                    unchanged_consensus_object.kind = Some(
                        unchanged_consensus_object::UnchangedConsensusObjectKind::ReadOnlyRoot
                            .into(),
                    );
                    unchanged_consensus_object.object_id = Some(object_id);
                    unchanged_consensus_object.version = Some(version);
                    unchanged_consensus_object.digest = Some(digest);

                    unchanged_consensus_objects.push(unchanged_consensus_object);
                }
            }

            if mask.contains(Self::GAS_OBJECT_FIELD.name) {
                let gas_object_id = value.gas_object().0.0.to_canonical_string(true);
                self.gas_object = changed_objects
                    .iter()
                    .find(|object| object.object_id() == gas_object_id)
                    .cloned();
            }

            if mask.contains(Self::CHANGED_OBJECTS_FIELD.name) {
                self.changed_objects = changed_objects;
            }

            if mask.contains(Self::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name) {
                self.unchanged_consensus_objects = unchanged_consensus_objects;
            }
        }
    }
}

//
// TransactionEffectsV2
//

impl Merge<&crate::effects::TransactionEffectsV2> for TransactionEffects {
    fn merge(
        &mut self,
        crate::effects::TransactionEffectsV2 {
            status,
            executed_epoch,
            gas_used,
            transaction_digest,
            gas_object_index,
            events_digest,
            dependencies,
            lamport_version,
            changed_objects,
            unchanged_consensus_objects,
            aux_data_digest,
        }: &crate::effects::TransactionEffectsV2,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(2);
        }

        if mask.contains(Self::STATUS_FIELD.name) {
            self.status = Some(status.clone().into());
        }

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = Some(*executed_epoch);
        }

        if mask.contains(Self::GAS_USED_FIELD.name) {
            self.gas_used = Some(gas_used.clone().into());
        }

        if mask.contains(Self::TRANSACTION_DIGEST_FIELD.name) {
            self.transaction_digest = Some(transaction_digest.to_string());
        }

        if mask.contains(Self::GAS_OBJECT_FIELD.name) {
            self.gas_object = gas_object_index
                .map(|index| {
                    changed_objects
                        .get(index as usize)
                        .cloned()
                        .map(|(id, change)| {
                            let mut message = ChangedObject::from(change);
                            message.object_id = Some(id.to_canonical_string(true));
                            message
                        })
                })
                .flatten();
        }

        if mask.contains(Self::EVENTS_DIGEST_FIELD.name) {
            self.events_digest = events_digest.map(|d| d.to_string());
        }

        if mask.contains(Self::DEPENDENCIES_FIELD.name) {
            self.dependencies = dependencies.iter().map(ToString::to_string).collect();
        }

        if mask.contains(Self::LAMPORT_VERSION_FIELD.name) {
            self.lamport_version = Some(lamport_version.value());
        }

        if mask.contains(Self::CHANGED_OBJECTS_FIELD.name) {
            self.changed_objects = changed_objects
                .clone()
                .into_iter()
                .map(|(id, change)| {
                    let mut message = ChangedObject::from(change);
                    message.object_id = Some(id.to_canonical_string(true));
                    message
                })
                .collect();
        }

        for object in self.changed_objects.iter_mut().chain(&mut self.gas_object) {
            if object.output_digest.is_some() && object.output_version.is_none() {
                object.output_version = Some(lamport_version.value());
            }
        }

        if mask.contains(Self::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name) {
            self.unchanged_consensus_objects = unchanged_consensus_objects
                .clone()
                .into_iter()
                .map(|(id, unchanged)| {
                    let mut message = UnchangedConsensusObject::from(unchanged);
                    message.object_id = Some(id.to_canonical_string(true));
                    message
                })
                .collect();
        }

        if mask.contains(Self::AUXILIARY_DATA_DIGEST_FIELD.name) {
            self.auxiliary_data_digest = aux_data_digest.map(|d| d.to_string());
        }
    }
}

//
// ChangedObject
//

impl From<crate::effects::EffectsObjectChange> for ChangedObject {
    fn from(value: crate::effects::EffectsObjectChange) -> Self {
        use crate::effects::ObjectIn;
        use crate::effects::ObjectOut;
        use changed_object::InputObjectState;
        use changed_object::OutputObjectState;

        let mut message = Self::default();

        // Input State
        let input_state = match value.input_state {
            ObjectIn::NotExist => InputObjectState::DoesNotExist,
            ObjectIn::Exist(((version, digest), owner)) => {
                message.input_version = Some(version.value());
                message.input_digest = Some(digest.to_string());
                message.input_owner = Some(owner.into());
                InputObjectState::Exists
            }
        };
        message.set_input_state(input_state);

        // Output State
        let output_state = match value.output_state {
            ObjectOut::NotExist => OutputObjectState::DoesNotExist,
            ObjectOut::ObjectWrite((digest, owner)) => {
                message.output_digest = Some(digest.to_string());
                message.output_owner = Some(owner.into());
                OutputObjectState::ObjectWrite
            }
            ObjectOut::PackageWrite((version, digest)) => {
                message.output_version = Some(version.value());
                message.output_digest = Some(digest.to_string());
                OutputObjectState::PackageWrite
            }
            ObjectOut::AccumulatorWriteV1(accumulator_write) => {
                message.set_accumulator_write(accumulator_write);
                OutputObjectState::AccumulatorWrite
            }
        };
        message.set_output_state(output_state);

        message.set_id_operation(value.id_operation.into());
        message
    }
}

impl From<crate::effects::AccumulatorWriteV1> for AccumulatorWrite {
    fn from(value: crate::effects::AccumulatorWriteV1) -> Self {
        use accumulator_write::AccumulatorOperation;

        let mut message = Self::default();

        message.set_address(value.address.address.to_string());
        message.set_accumulator_type(value.address.ty.to_canonical_string(true));
        message.set_operation(match value.operation {
            crate::effects::AccumulatorOperation::Merge => AccumulatorOperation::Merge,
            crate::effects::AccumulatorOperation::Split => AccumulatorOperation::Split,
        });
        match value.value {
            crate::effects::AccumulatorValue::Integer(value) => message.set_value(value),
            //TODO unsupported value types
            crate::effects::AccumulatorValue::IntegerTuple(_, _)
            | crate::effects::AccumulatorValue::EventDigest(_) => {}
        }

        message
    }
}

//
// IdOperation
//

impl From<crate::effects::IDOperation> for changed_object::IdOperation {
    fn from(value: crate::effects::IDOperation) -> Self {
        use crate::effects::IDOperation as I;

        match value {
            I::None => Self::None,
            I::Created => Self::Created,
            I::Deleted => Self::Deleted,
        }
    }
}

//
// UnchangedConsensusObject
//

impl From<crate::effects::UnchangedConsensusKind> for UnchangedConsensusObject {
    fn from(value: crate::effects::UnchangedConsensusKind) -> Self {
        use crate::effects::UnchangedConsensusKind as K;
        use unchanged_consensus_object::UnchangedConsensusObjectKind;

        let mut message = Self::default();

        let kind = match value {
            K::ReadOnlyRoot((version, digest)) => {
                message.version = Some(version.value());
                message.digest = Some(digest.to_string());
                UnchangedConsensusObjectKind::ReadOnlyRoot
            }
            K::MutateConsensusStreamEnded(version) => {
                message.version = Some(version.value());
                UnchangedConsensusObjectKind::MutateConsensusStreamEnded
            }
            K::ReadConsensusStreamEnded(version) => {
                message.version = Some(version.value());
                UnchangedConsensusObjectKind::ReadConsensusStreamEnded
            }
            K::Cancelled(version) => {
                message.version = Some(version.value());
                UnchangedConsensusObjectKind::Canceled
            }
            K::PerEpochConfig => UnchangedConsensusObjectKind::PerEpochConfig,
            // PerEpochConfigWithSequenceNumber { version } => {
            //     message.version = Some(version);
            //     UnchangedSharedObjectKind::PerEpochConfig
            // }
        };

        message.set_kind(kind);
        message
    }
}

//
// TransactionChecks
//

impl From<simulate_transaction_request::TransactionChecks>
    for crate::transaction_executor::TransactionChecks
{
    fn from(value: simulate_transaction_request::TransactionChecks) -> Self {
        match value {
            simulate_transaction_request::TransactionChecks::Enabled => Self::Enabled,
            simulate_transaction_request::TransactionChecks::Disabled => Self::Disabled,
            // Default to enabled
            _ => Self::Enabled,
        }
    }
}

//
// Coin-related conversions
//

impl From<crate::coin_registry::MetadataCapState> for coin_metadata::MetadataCapState {
    fn from(value: crate::coin_registry::MetadataCapState) -> Self {
        match value {
            crate::coin_registry::MetadataCapState::Claimed(_) => {
                coin_metadata::MetadataCapState::Claimed
            }
            crate::coin_registry::MetadataCapState::Unclaimed => {
                coin_metadata::MetadataCapState::Unclaimed
            }
            crate::coin_registry::MetadataCapState::Deleted => {
                coin_metadata::MetadataCapState::Deleted
            }
        }
    }
}

impl From<&crate::coin_registry::Currency> for CoinMetadata {
    fn from(value: &crate::coin_registry::Currency) -> Self {
        let mut metadata = CoinMetadata::default();
        metadata.id = Some(sui_sdk_types::Address::from(value.id.into_bytes()).to_string());
        metadata.decimals = Some(value.decimals.into());
        metadata.name = Some(value.name.clone());
        metadata.symbol = Some(value.symbol.clone());
        metadata.description = Some(value.description.clone());
        metadata.icon_url = Some(value.icon_url.clone());

        match &value.metadata_cap_id {
            crate::coin_registry::MetadataCapState::Claimed(id) => {
                metadata.metadata_cap_state = Some(coin_metadata::MetadataCapState::Claimed as i32);
                metadata.metadata_cap_id = Some(sui_sdk_types::Address::from(*id).to_string());
            }
            crate::coin_registry::MetadataCapState::Unclaimed => {
                metadata.metadata_cap_state =
                    Some(coin_metadata::MetadataCapState::Unclaimed as i32);
            }
            crate::coin_registry::MetadataCapState::Deleted => {
                metadata.metadata_cap_state = Some(coin_metadata::MetadataCapState::Deleted as i32);
            }
        }

        metadata
    }
}

impl From<crate::coin::CoinMetadata> for CoinMetadata {
    fn from(value: crate::coin::CoinMetadata) -> Self {
        let mut metadata = CoinMetadata::default();
        metadata.id = Some(sui_sdk_types::Address::from(value.id.id.bytes).to_string());
        metadata.decimals = Some(value.decimals.into());
        metadata.name = Some(value.name);
        metadata.symbol = Some(value.symbol);
        metadata.description = Some(value.description);
        metadata.icon_url = value.icon_url;
        metadata
    }
}

impl From<crate::coin_registry::SupplyState> for coin_treasury::SupplyState {
    fn from(value: crate::coin_registry::SupplyState) -> Self {
        match value {
            crate::coin_registry::SupplyState::Fixed(_) => coin_treasury::SupplyState::Fixed,
            crate::coin_registry::SupplyState::BurnOnly(_) => coin_treasury::SupplyState::BurnOnly,
            crate::coin_registry::SupplyState::Unknown => coin_treasury::SupplyState::Unknown,
        }
    }
}

impl From<crate::coin::TreasuryCap> for CoinTreasury {
    fn from(value: crate::coin::TreasuryCap) -> Self {
        let mut treasury = CoinTreasury::default();
        treasury.id = Some(sui_sdk_types::Address::from(value.id.id.bytes).to_string());
        treasury.total_supply = Some(value.total_supply.value);
        treasury
    }
}

impl From<&crate::coin_registry::RegulatedState> for RegulatedCoinMetadata {
    fn from(value: &crate::coin_registry::RegulatedState) -> Self {
        let mut regulated = RegulatedCoinMetadata::default();

        match value {
            crate::coin_registry::RegulatedState::Regulated {
                cap,
                allow_global_pause,
                variant,
            } => {
                regulated.deny_cap_object = Some(sui_sdk_types::Address::from(*cap).to_string());
                regulated.allow_global_pause = *allow_global_pause;
                regulated.variant = Some(*variant as u32);
                regulated.coin_regulated_state =
                    Some(regulated_coin_metadata::CoinRegulatedState::Regulated as i32);
            }
            crate::coin_registry::RegulatedState::Unregulated => {
                regulated.coin_regulated_state =
                    Some(regulated_coin_metadata::CoinRegulatedState::Unregulated as i32);
            }
            crate::coin_registry::RegulatedState::Unknown => {
                regulated.coin_regulated_state =
                    Some(regulated_coin_metadata::CoinRegulatedState::Unknown as i32);
            }
        }

        regulated
    }
}

impl From<crate::coin_registry::RegulatedState> for RegulatedCoinMetadata {
    fn from(value: crate::coin_registry::RegulatedState) -> Self {
        (&value).into()
    }
}

impl From<crate::coin::RegulatedCoinMetadata> for RegulatedCoinMetadata {
    fn from(value: crate::coin::RegulatedCoinMetadata) -> Self {
        let mut message = RegulatedCoinMetadata::default();
        message.id = Some(sui_sdk_types::Address::from(value.id.id.bytes).to_string());
        message.coin_metadata_object =
            Some(sui_sdk_types::Address::from(value.coin_metadata_object.bytes).to_string());
        message.deny_cap_object =
            Some(sui_sdk_types::Address::from(value.deny_cap_object.bytes).to_string());
        message.coin_regulated_state =
            Some(regulated_coin_metadata::CoinRegulatedState::Regulated as i32);
        message
    }
}

impl TryFrom<&ObjectSet> for crate::full_checkpoint_content::ObjectSet {
    type Error = TryFromProtoError;

    fn try_from(value: &ObjectSet) -> Result<Self, Self::Error> {
        let mut objects = Self::default();

        for o in value.objects() {
            objects.insert(
                o.bcs()
                    .deserialize()
                    .map_err(|e| TryFromProtoError::invalid("object.bcs", e))?,
            );
        }

        Ok(objects)
    }
}
