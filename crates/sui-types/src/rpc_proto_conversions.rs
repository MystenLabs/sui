// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Module for conversions from sui-core types to rpc protos

use crate::crypto::SuiSignature;
use crate::message_envelope::Message as _;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2beta2::*;

//
// CheckpointSummary
//

impl From<crate::full_checkpoint_content::CheckpointData> for Checkpoint {
    fn from(checkpoint_data: crate::full_checkpoint_content::CheckpointData) -> Self {
        Self::merge_from(checkpoint_data, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::full_checkpoint_content::CheckpointData> for Checkpoint {
    fn merge(
        &mut self,
        source: crate::full_checkpoint_content::CheckpointData,
        mask: &FieldMaskTree,
    ) {
        let sequence_number = source.checkpoint_summary.sequence_number;
        let timestamp_ms = source.checkpoint_summary.timestamp_ms;

        let summary = source.checkpoint_summary.data();
        let signature = source.checkpoint_summary.auth_sig();

        self.merge(summary, mask);
        self.merge(signature.clone(), mask);

        if mask.contains(Checkpoint::CONTENTS_FIELD.name) {
            self.merge(&source.checkpoint_contents, mask);
        }

        if let Some(submask) = mask.subtree(Checkpoint::TRANSACTIONS_FIELD.name) {
            self.transactions = source
                .transactions
                .into_iter()
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

impl Merge<crate::full_checkpoint_content::CheckpointTransaction> for ExecutedTransaction {
    fn merge(
        &mut self,
        source: crate::full_checkpoint_content::CheckpointTransaction,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(ExecutedTransaction::DIGEST_FIELD) {
            self.digest = Some(source.transaction.digest().to_string());
        }

        let (transaction_data, signatures) = {
            let sender_signed = source.transaction.into_data().into_inner();
            (
                sender_signed.intent_message.value,
                sender_signed.tx_signatures,
            )
        };

        if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD) {
            self.transaction = Some(Transaction::merge_from(transaction_data, &submask));
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD) {
            self.signatures = signatures
                .into_iter()
                .map(|s| UserSignature::merge_from(s, &submask))
                .collect();
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD) {
            self.effects = Some(TransactionEffects::merge_from(&source.effects, &submask));
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD) {
            self.events = source
                .events
                .map(|events| TransactionEvents::merge_from(events, &submask));
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::INPUT_OBJECTS_FIELD) {
            self.input_objects = source
                .input_objects
                .into_iter()
                .map(|o| Object::merge_from(o, &submask))
                .collect();
        }

        if let Some(submask) = mask.subtree(ExecutedTransaction::OUTPUT_OBJECTS_FIELD) {
            self.output_objects = source
                .output_objects
                .into_iter()
                .map(|o| Object::merge_from(o, &submask))
                .collect();
        }
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
        Self {
            computation_cost: Some(computation_cost),
            storage_cost: Some(storage_cost),
            storage_rebate: Some(storage_rebate),
            non_refundable_storage_fee: Some(non_refundable_storage_fee),
        }
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
        Self {
            next_epoch_committee: next_epoch_committee
                .into_iter()
                .map(|(name, weight)| ValidatorCommitteeMember {
                    public_key: Some(name.0.to_vec().into()),
                    weight: Some(weight),
                })
                .collect(),
            next_epoch_protocol_version: Some(next_epoch_protocol_version.as_u64()),
            epoch_commitments: epoch_commitments.into_iter().map(Into::into).collect(),
        }
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
                .map(|(digests, sigs)| CheckpointedTransactionInfo {
                    transaction: Some(digests.transaction.to_string()),
                    effects: Some(digests.effects.to_string()),
                    signatures: sigs.into_iter().map(Into::into).collect(),
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
        Self::merge_from(value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::event::Event> for Event {
    fn merge(&mut self, source: crate::event::Event, mask: &FieldMaskTree) {
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
            self.contents = Some(Bcs {
                name: Some(source.type_.to_canonical_string(true)),
                value: Some(source.contents.into()),
            });
        }
    }
}

//
// TransactionEvents
//

impl From<crate::effects::TransactionEvents> for TransactionEvents {
    fn from(value: crate::effects::TransactionEvents) -> Self {
        Self::merge_from(value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::effects::TransactionEvents> for TransactionEvents {
    fn merge(&mut self, source: crate::effects::TransactionEvents, mask: &FieldMaskTree) {
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
                .into_iter()
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
            .map(|entry| ValidatorReportRecord {
                reported: Some(entry.key.to_string()),
                reporters: entry
                    .value
                    .contents
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            })
            .collect();

        Self {
            version: Some(system_state_version),
            epoch: Some(epoch),
            protocol_version: Some(protocol_version),
            validators: Some(validators.into()),
            storage_fund: Some(storage_fund.into()),
            parameters: Some(parameters.into()),
            reference_gas_price: Some(reference_gas_price),
            validator_report_records,
            stake_subsidy: Some(stake_subsidy.into()),
            safe_mode: Some(safe_mode),
            safe_mode_storage_rewards: Some(safe_mode_storage_rewards.value()),
            safe_mode_computation_rewards: Some(safe_mode_computation_rewards.value()),
            safe_mode_storage_rebates: Some(safe_mode_storage_rebates),
            safe_mode_non_refundable_storage_fee: Some(safe_mode_non_refundable_storage_fee),
            epoch_start_timestamp_ms: Some(epoch_start_timestamp_ms),
            extra_fields: Some(extra_fields.into()),
        }
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
            .map(|entry| ValidatorReportRecord {
                reported: Some(entry.key.to_string()),
                reporters: entry
                    .value
                    .contents
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            })
            .collect();

        Self {
            version: Some(system_state_version),
            epoch: Some(epoch),
            protocol_version: Some(protocol_version),
            validators: Some(validators.into()),
            storage_fund: Some(storage_fund.into()),
            parameters: Some(parameters.into()),
            reference_gas_price: Some(reference_gas_price),
            validator_report_records,
            stake_subsidy: Some(stake_subsidy.into()),
            safe_mode: Some(safe_mode),
            safe_mode_storage_rewards: Some(safe_mode_storage_rewards.value()),
            safe_mode_computation_rewards: Some(safe_mode_computation_rewards.value()),
            safe_mode_storage_rebates: Some(safe_mode_storage_rebates),
            safe_mode_non_refundable_storage_fee: Some(safe_mode_non_refundable_storage_fee),
            epoch_start_timestamp_ms: Some(epoch_start_timestamp_ms),
            extra_fields: Some(extra_fields.into()),
        }
    }
}

impl From<crate::collection_types::Bag> for MoveTable {
    fn from(crate::collection_types::Bag { id, size }: crate::collection_types::Bag) -> Self {
        Self {
            id: Some(id.id.bytes.to_canonical_string(true)),
            size: Some(size),
        }
    }
}

impl From<crate::collection_types::Table> for MoveTable {
    fn from(crate::collection_types::Table { id, size }: crate::collection_types::Table) -> Self {
        Self {
            id: Some(id.to_canonical_string(true)),
            size: Some(size),
        }
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
        Self {
            balance: Some(balance.value()),
            distribution_counter: Some(distribution_counter),
            current_distribution_amount: Some(current_distribution_amount),
            stake_subsidy_period_length: Some(stake_subsidy_period_length),
            stake_subsidy_decrease_rate: Some(stake_subsidy_decrease_rate.into()),
            extra_fields: Some(extra_fields.into()),
        }
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
        Self {
            epoch_duration_ms: Some(epoch_duration_ms),
            stake_subsidy_start_epoch: Some(stake_subsidy_start_epoch),
            min_validator_count: None,
            max_validator_count: Some(max_validator_count),
            min_validator_joining_stake: Some(min_validator_joining_stake),
            validator_low_stake_threshold: Some(validator_low_stake_threshold),
            validator_very_low_stake_threshold: Some(validator_very_low_stake_threshold),
            validator_low_stake_grace_period: Some(validator_low_stake_grace_period),
            extra_fields: Some(extra_fields.into()),
        }
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
        Self {
            epoch_duration_ms: Some(epoch_duration_ms),
            stake_subsidy_start_epoch: Some(stake_subsidy_start_epoch),
            min_validator_count: Some(min_validator_count),
            max_validator_count: Some(max_validator_count),
            min_validator_joining_stake: Some(min_validator_joining_stake),
            validator_low_stake_threshold: Some(validator_low_stake_threshold),
            validator_very_low_stake_threshold: Some(validator_very_low_stake_threshold),
            validator_low_stake_grace_period: Some(validator_low_stake_grace_period),
            extra_fields: Some(extra_fields.into()),
        }
    }
}

impl From<crate::sui_system_state::sui_system_state_inner_v1::StorageFundV1> for StorageFund {
    fn from(
        crate::sui_system_state::sui_system_state_inner_v1::StorageFundV1 {
            total_object_storage_rebates,
            non_refundable_balance,
        }: crate::sui_system_state::sui_system_state_inner_v1::StorageFundV1,
    ) -> Self {
        Self {
            total_object_storage_rebates: Some(total_object_storage_rebates.value()),
            non_refundable_balance: Some(non_refundable_balance.value()),
        }
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
        Self {
            total_stake: Some(total_stake),
            active_validators: active_validators.into_iter().map(Into::into).collect(),
            pending_active_validators: Some(pending_active_validators.into()),
            pending_removals,
            staking_pool_mappings: Some(staking_pool_mappings.into()),
            inactive_validators: Some(inactive_validators.into()),
            validator_candidates: Some(validator_candidates.into()),
            at_risk_validators,
            extra_fields: Some(extra_fields.into()),
        }
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
        Self {
            id: Some(id.to_canonical_string(true)),
            activation_epoch,
            deactivation_epoch,
            sui_balance: Some(sui_balance),
            rewards_pool: Some(rewards_pool.value()),
            pool_token_balance: Some(pool_token_balance),
            exchange_rates: Some(exchange_rates.into()),
            pending_stake: Some(pending_stake),
            pending_total_sui_withdraw: Some(pending_total_sui_withdraw),
            pending_pool_token_withdraw: Some(pending_pool_token_withdraw),
            extra_fields: Some(extra_fields.into()),
        }
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
        Self {
            name: Some(name),
            address: Some(sui_address.to_string()),
            description: Some(description),
            image_url: Some(image_url),
            project_url: Some(project_url),
            protocol_public_key: Some(protocol_pubkey_bytes.into()),
            proof_of_possession: Some(proof_of_possession_bytes.into()),
            network_public_key: Some(network_pubkey_bytes.into()),
            worker_public_key: Some(worker_pubkey_bytes.into()),
            network_address: Some(net_address),
            p2p_address: Some(p2p_address),
            primary_address: Some(primary_address),
            worker_address: Some(worker_address),
            next_epoch_protocol_public_key: next_epoch_protocol_pubkey_bytes.map(Into::into),
            next_epoch_proof_of_possession: next_epoch_proof_of_possession.map(Into::into),
            next_epoch_network_public_key: next_epoch_network_pubkey_bytes.map(Into::into),
            next_epoch_worker_public_key: next_epoch_worker_pubkey_bytes.map(Into::into),
            next_epoch_network_address: next_epoch_net_address,
            next_epoch_p2p_address,
            next_epoch_primary_address,
            next_epoch_worker_address,
            metadata_extra_fields: Some(metadata_extra_fields.into()),
            voting_power: Some(voting_power),
            operation_cap_id: Some(operation_cap_id.bytes.to_canonical_string(true)),
            gas_price: Some(gas_price),
            staking_pool: Some(staking_pool.into()),
            commission_rate: Some(commission_rate),
            next_epoch_stake: Some(next_epoch_stake),
            next_epoch_gas_price: Some(next_epoch_gas_price),
            next_epoch_commission_rate: Some(next_epoch_commission_rate),
            extra_fields: Some(extra_fields.into()),
        }
    }
}

//
// ExecutionStatus
//

impl From<crate::execution_status::ExecutionStatus> for ExecutionStatus {
    fn from(value: crate::execution_status::ExecutionStatus) -> Self {
        match value {
            crate::execution_status::ExecutionStatus::Success => Self {
                success: Some(true),
                error: None,
            },
            crate::execution_status::ExecutionStatus::Failure { error, command } => {
                let description = if let Some(command) = command {
                    format!("{error:?} in command {command}")
                } else {
                    format!("{error:?}")
                };
                let mut error_message = ExecutionError::from(error);
                error_message.command = command.map(|i| i as u64);
                error_message.description = Some(description);
                Self {
                    success: Some(false),
                    error: Some(error_message),
                }
            }
        }
    }
}

//
// ExecutionError
//

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
                message.error_details = Some(ErrorDetails::SizeError(SizeError {
                    size: Some(object_size),
                    max_size: Some(max_object_size),
                }));
                ExecutionErrorKind::ObjectTooBig
            }
            E::MovePackageTooBig {
                object_size,
                max_object_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(SizeError {
                    size: Some(object_size),
                    max_size: Some(max_object_size),
                }));
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
                    ErrorDetails::Abort(MoveAbort {
                        location: Some(l.into()),
                        ..Default::default()
                    })
                });
                ExecutionErrorKind::MovePrimitiveRuntimeError
            }
            E::MoveAbort(location, code) => {
                message.error_details = Some(ErrorDetails::Abort(MoveAbort {
                    abort_code: Some(code),
                    location: Some(location.into()),
                    clever_error: None,
                }));
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
                let type_argument_error = TypeArgumentError {
                    type_argument: Some(argument_idx.into()),
                    kind: Some(type_argument_error::TypeArgumentErrorKind::from(kind).into()),
                };
                message.error_details = Some(ErrorDetails::TypeArgumentError(type_argument_error));
                ExecutionErrorKind::TypeArgumentError
            }
            E::UnusedValueWithoutDrop {
                result_idx,
                secondary_idx,
            } => {
                message.error_details = Some(ErrorDetails::IndexError(IndexError {
                    index: Some(result_idx.into()),
                    subresult: Some(secondary_idx.into()),
                }));
                ExecutionErrorKind::UnusedValueWithoutDrop
            }
            E::InvalidPublicFunctionReturnType { idx } => {
                message.error_details = Some(ErrorDetails::IndexError(IndexError {
                    index: Some(idx.into()),
                    subresult: None,
                }));
                ExecutionErrorKind::InvalidPublicFunctionReturnType
            }
            E::InvalidTransferObject => ExecutionErrorKind::InvalidTransferObject,
            E::EffectsTooLarge {
                current_size,
                max_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(SizeError {
                    size: Some(current_size),
                    max_size: Some(max_size),
                }));
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
                message.error_details = Some(ErrorDetails::SizeError(SizeError {
                    size: Some(current_size),
                    max_size: Some(max_size),
                }));

                ExecutionErrorKind::WrittenObjectsTooLarge
            }
            E::CertificateDenied => ExecutionErrorKind::CertificateDenied,
            E::SuiMoveVerificationTimedout => ExecutionErrorKind::SuiMoveVerificationTimedout,
            E::SharedObjectOperationNotAllowed => {
                ExecutionErrorKind::SharedObjectOperationNotAllowed
            }
            E::InputObjectDeleted => ExecutionErrorKind::InputObjectDeleted,
            E::ExecutionCancelledDueToSharedObjectCongestion { congested_objects } => {
                message.error_details = Some(ErrorDetails::CongestedObjects(CongestedObjects {
                    objects: congested_objects
                        .0
                        .iter()
                        .map(|o| o.to_canonical_string(true))
                        .collect(),
                }));

                ExecutionErrorKind::ExecutionCanceledDueToSharedObjectCongestion
            }
            E::AddressDeniedForCoin { address, coin_type } => {
                message.error_details = Some(ErrorDetails::CoinDenyListError(CoinDenyListError {
                    address: Some(address.to_string()),
                    coin_type: Some(coin_type),
                }));
                ExecutionErrorKind::AddressDeniedForCoin
            }
            E::CoinTypeGlobalPause { coin_type } => {
                message.error_details = Some(ErrorDetails::CoinDenyListError(CoinDenyListError {
                    address: None,
                    coin_type: Some(coin_type),
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
                message.error_details = Some(ErrorDetails::SizeError(SizeError {
                    size: Some(value_size),
                    max_size: Some(max_scaled_size),
                }));

                ExecutionErrorKind::MoveVectorElemTooBig
            }
            E::MoveRawValueTooBig {
                value_size,
                max_scaled_size,
            } => {
                message.error_details = Some(ErrorDetails::SizeError(SizeError {
                    size: Some(value_size),
                    max_size: Some(max_scaled_size),
                }));
                ExecutionErrorKind::MoveRawValueTooBig
            }
            E::InvalidLinkage => ExecutionErrorKind::InvalidLinkage,
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
                message.index_error = Some(IndexError {
                    index: Some(idx.into()),
                    subresult: None,
                });
                CommandArgumentErrorKind::IndexOutOfBounds
            }
            E::SecondaryIndexOutOfBounds {
                result_idx,
                secondary_idx,
            } => {
                message.index_error = Some(IndexError {
                    index: Some(result_idx.into()),
                    subresult: Some(secondary_idx.into()),
                });
                CommandArgumentErrorKind::SecondaryIndexOutOfBounds
            }
            E::InvalidResultArity { result_idx } => {
                message.index_error = Some(IndexError {
                    index: Some(result_idx.into()),
                    subresult: None,
                });
                CommandArgumentErrorKind::InvalidResultArity
            }
            E::InvalidGasCoinUsage => CommandArgumentErrorKind::InvalidGasCoinUsage,
            E::InvalidValueUsage => CommandArgumentErrorKind::InvalidValueUsage,
            E::InvalidObjectByValue => CommandArgumentErrorKind::InvalidObjectByValue,
            E::InvalidObjectByMutRef => CommandArgumentErrorKind::InvalidObjectByMutRef,
            E::SharedObjectOperationNotAllowed => {
                CommandArgumentErrorKind::SharedObjectOperationNotAllowed
            }
            E::InvalidArgumentArity => CommandArgumentErrorKind::InvalidArgumentArity,

            //TODO
            E::InvalidTransferObject => CommandArgumentErrorKind::Unknown,
            //TODO
            E::InvalidMakeMoveVecNonObjectArgument => CommandArgumentErrorKind::Unknown,
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
        Self {
            package: Some(value.module.address().to_canonical_string(true)),
            module: Some(value.module.name().to_string()),
            function: Some(value.function.into()),
            instruction: Some(value.instruction.into()),
            function_name: value.function_name.map(|name| name.to_string()),
        }
    }
}

//
// AuthorityQuorumSignInfo aka ValidatorAggregatedSignature
//

impl<const T: bool> From<crate::crypto::AuthorityQuorumSignInfo<T>>
    for ValidatorAggregatedSignature
{
    fn from(value: crate::crypto::AuthorityQuorumSignInfo<T>) -> Self {
        Self {
            epoch: Some(value.epoch),
            signature: Some(value.signature.as_ref().to_vec().into()),
            bitmap: value.signers_map.iter().collect(),
        }
    }
}

//
// ValidatorCommittee
//

impl From<crate::committee::Committee> for ValidatorCommittee {
    fn from(value: crate::committee::Committee) -> Self {
        Self {
            epoch: Some(value.epoch),
            members: value
                .voting_rights
                .into_iter()
                .map(|(name, weight)| ValidatorCommitteeMember {
                    public_key: Some(name.0.to_vec().into()),
                    weight: Some(weight),
                })
                .collect(),
        }
    }
}

//
// ZkLoginAuthenticator
//

impl From<crate::zk_login_authenticator::ZkLoginAuthenticator> for ZkLoginAuthenticator {
    fn from(value: crate::zk_login_authenticator::ZkLoginAuthenticator) -> Self {
        let inputs = ZkLoginInputs {
            proof_points: None,       // TODO expose in fastcrypto
            iss_base64_details: None, // TODO expose in fastcrypto
            header_base64: None,      // TODO expose in fastcrypto
            address_seed: Some(value.inputs.get_address_seed().to_string()),
        };
        Self {
            inputs: Some(inputs),
            max_epoch: Some(value.get_max_epoch()),
            signature: Some(value.user_signature.into()),
        }
    }
}

//
// ZkLoginPublicIdentifier
//

impl From<&crate::crypto::ZkLoginPublicIdentifier> for ZkLoginPublicIdentifier {
    fn from(_value: &crate::crypto::ZkLoginPublicIdentifier) -> Self {
        Self {
            iss: None,          // TODO expose
            address_seed: None, // TODO expose
        }
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
        let scheme: SignatureScheme = value.scheme().into();
        let signature = value.signature_bytes();
        let public_key = value.public_key_bytes();

        Self {
            scheme: Some(scheme.into()),
            signature: Some(signature.to_vec().into()),
            public_key: Some(public_key.to_vec().into()),
        }
    }
}

//
// PasskeyAuthenticator
//

impl From<crate::passkey_authenticator::PasskeyAuthenticator> for PasskeyAuthenticator {
    fn from(value: crate::passkey_authenticator::PasskeyAuthenticator) -> Self {
        Self {
            authenticator_data: Some(value.authenticator_data().to_vec().into()),
            client_data_json: Some(value.client_data_json().to_owned()),
            signature: Some(value.signature().into()),
        }
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
        Self {
            members: value
                .pubkeys()
                .iter()
                .map(|(pk, weight)| MultisigMember {
                    public_key: Some(pk.into()),
                    weight: Some((*weight).into()),
                })
                .collect(),
            threshold: Some((*value.threshold()).into()),
        }
    }
}

impl From<&crate::multisig_legacy::MultiSigPublicKeyLegacy> for MultisigCommittee {
    fn from(value: &crate::multisig_legacy::MultiSigPublicKeyLegacy) -> Self {
        Self {
            members: value
                .pubkeys()
                .iter()
                .map(|(pk, weight)| MultisigMember {
                    public_key: Some(pk.into()),
                    weight: Some((*weight).into()),
                })
                .collect(),
            threshold: Some((*value.threshold()).into()),
        }
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
        Self {
            signatures: value.get_sigs().iter().map(Into::into).collect(),
            bitmap: None,
            legacy_bitmap: value.get_bitmap().iter().collect(),
            committee: Some(value.get_pk().into()),
        }
    }
}

impl From<&crate::multisig::MultiSig> for MultisigAggregatedSignature {
    fn from(value: &crate::multisig::MultiSig) -> Self {
        Self {
            signatures: value.get_sigs().iter().map(Into::into).collect(),
            bitmap: Some(value.get_bitmap().into()),
            legacy_bitmap: Default::default(),
            committee: Some(value.get_pk().into()),
        }
    }
}

//
// UserSignature
//

impl From<crate::signature::GenericSignature> for UserSignature {
    fn from(value: crate::signature::GenericSignature) -> Self {
        Self::merge_from(value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::signature::GenericSignature> for UserSignature {
    fn merge(&mut self, source: crate::signature::GenericSignature, mask: &FieldMaskTree) {
        use user_signature::Signature;

        if mask.contains(Self::BCS_FIELD) {
            self.bcs = Some(Bcs {
                name: Some("UserSignatureBytes".to_owned()),
                value: Some(source.as_ref().to_vec().into()),
            });
        }

        let scheme = match source {
            crate::signature::GenericSignature::MultiSig(ref multi_sig) => {
                if mask.contains(Self::MULTISIG_FIELD) {
                    self.signature = Some(Signature::Multisig(multi_sig.into()));
                }
                SignatureScheme::Multisig
            }
            crate::signature::GenericSignature::MultiSigLegacy(ref multi_sig_legacy) => {
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
        Self {
            address: Some(value.address.to_string()),
            coin_type: Some(value.coin_type.to_canonical_string(true)),
            amount: Some(value.amount.to_string()),
        }
    }
}

//
// Object
//

pub const PACKAGE_TYPE: &str = "package";

impl From<crate::object::Object> for Object {
    fn from(value: crate::object::Object) -> Self {
        Self::merge_from(value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::object::Object> for Object {
    fn merge(&mut self, source: crate::object::Object, mask: &FieldMaskTree) {
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
            self.contents = Some(Bcs {
                name: Some(source.type_().to_canonical_string(true)),
                value: Some(source.contents().to_vec().into()),
            });
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
            self.package = Some(Package {
                modules: source
                    .serialized_module_map()
                    .iter()
                    .map(|(name, contents)| Module {
                        name: Some(name.to_string()),
                        contents: Some(contents.clone().into()),
                        ..Default::default()
                    })
                    .collect(),
                type_origins: source
                    .type_origin_table()
                    .clone()
                    .into_iter()
                    .map(Into::into)
                    .collect(),
                linkage: source
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
                            Linkage {
                                original_id: Some(original_id.to_canonical_string(true)),
                                upgraded_id: Some(upgraded_id.to_canonical_string(true)),
                                upgraded_version: Some(upgraded_version.value()),
                            }
                        },
                    )
                    .collect(),

                ..Default::default()
            })
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
        Self {
            module_name: Some(value.module_name.to_string()),
            datatype_name: Some(value.datatype_name.to_string()),
            package_id: Some(value.package.to_canonical_string(true)),
        }
    }
}

//
// GenesisObject
//

impl From<crate::transaction::GenesisObject> for Object {
    fn from(value: crate::transaction::GenesisObject) -> Self {
        let crate::transaction::GenesisObject::RawObject { data, owner } = value;
        let mut message = Self {
            owner: Some(owner.into()),
            ..Default::default()
        };

        message.merge(&data, &FieldMaskTree::new_wildcard());

        message
    }
}

//
// ObjectReference
//

fn object_ref_to_proto(value: crate::base_types::ObjectRef) -> ObjectReference {
    let (object_id, version, digest) = value;
    ObjectReference {
        object_id: Some(object_id.to_canonical_string(true)),
        version: Some(version.value()),
        digest: Some(digest.to_string()),
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
        Self::merge_from(value, &FieldMaskTree::new_wildcard())
    }
}

impl Merge<crate::transaction::TransactionData> for Transaction {
    fn merge(&mut self, source: crate::transaction::TransactionData, mask: &FieldMaskTree) {
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
            self.kind = Some(source.kind.into());
        }

        if mask.contains(Self::SENDER_FIELD.name) {
            self.sender = Some(source.sender.to_string());
        }

        if mask.contains(Self::GAS_PAYMENT_FIELD.name) {
            self.gas_payment = Some(source.gas_data.into());
        }

        if mask.contains(Self::EXPIRATION_FIELD.name) {
            self.expiration = Some(source.expiration.into());
        }
    }
}

//
// GasPayment
//

impl From<crate::transaction::GasData> for GasPayment {
    fn from(value: crate::transaction::GasData) -> Self {
        Self {
            objects: value.payment.into_iter().map(object_ref_to_proto).collect(),
            owner: Some(value.owner.to_string()),
            price: Some(value.price),
            budget: Some(value.budget),
        }
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
            TransactionExpirationKind::Unknown => return Err("unknown TransactionExpirationKind"),
            TransactionExpirationKind::None => Self::None,
            TransactionExpirationKind::Epoch => Self::Epoch(value.epoch()),
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

        let kind = match value {
            K::ProgrammableTransaction(ptb) => Kind::ProgrammableTransaction(ptb.into()),
            K::ProgrammableSystemTransaction(ptb) => {
                Kind::ProgrammableSystemTransaction(ptb.into())
            }
            K::ChangeEpoch(change_epoch) => Kind::ChangeEpoch(change_epoch.into()),
            K::Genesis(genesis) => Kind::Genesis(genesis.into()),
            K::ConsensusCommitPrologue(prologue) => {
                Kind::ConsensusCommitPrologueV1(prologue.into())
            }
            K::AuthenticatorStateUpdate(update) => Kind::AuthenticatorStateUpdate(update.into()),
            K::EndOfEpochTransaction(transactions) => Kind::EndOfEpoch(EndOfEpochTransaction {
                transactions: transactions.into_iter().map(Into::into).collect(),
            }),
            K::RandomnessStateUpdate(update) => Kind::RandomnessStateUpdate(update.into()),
            K::ConsensusCommitPrologueV2(prologue) => {
                Kind::ConsensusCommitPrologueV2(prologue.into())
            }
            K::ConsensusCommitPrologueV3(prologue) => {
                Kind::ConsensusCommitPrologueV3(prologue.into())
            }
            K::ConsensusCommitPrologueV4(prologue) => {
                Kind::ConsensusCommitPrologueV4(prologue.into())
            }
        };

        Self { kind: Some(kind) }
    }
}

//
// ConsensusCommitPrologue
//

impl From<crate::messages_consensus::ConsensusCommitPrologue> for ConsensusCommitPrologue {
    fn from(value: crate::messages_consensus::ConsensusCommitPrologue) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            commit_timestamp: Some(sui_rpc::proto::timestamp_ms_to_proto(
                value.commit_timestamp_ms,
            )),
            consensus_commit_digest: None,
            sub_dag_index: None,
            consensus_determined_version_assignments: None,
            additional_state_digest: None,
        }
    }
}

impl From<crate::messages_consensus::ConsensusCommitPrologueV2> for ConsensusCommitPrologue {
    fn from(value: crate::messages_consensus::ConsensusCommitPrologueV2) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            commit_timestamp: Some(sui_rpc::proto::timestamp_ms_to_proto(
                value.commit_timestamp_ms,
            )),
            consensus_commit_digest: Some(value.consensus_commit_digest.to_string()),
            sub_dag_index: None,
            consensus_determined_version_assignments: None,
            additional_state_digest: None,
        }
    }
}

impl From<crate::messages_consensus::ConsensusCommitPrologueV3> for ConsensusCommitPrologue {
    fn from(value: crate::messages_consensus::ConsensusCommitPrologueV3) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            commit_timestamp: Some(sui_rpc::proto::timestamp_ms_to_proto(
                value.commit_timestamp_ms,
            )),
            consensus_commit_digest: Some(value.consensus_commit_digest.to_string()),
            sub_dag_index: value.sub_dag_index,
            consensus_determined_version_assignments: Some(
                value.consensus_determined_version_assignments.into(),
            ),
            additional_state_digest: None,
        }
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
        Self {
            epoch: Some(epoch),
            round: Some(round),
            commit_timestamp: Some(sui_rpc::proto::timestamp_ms_to_proto(commit_timestamp_ms)),
            consensus_commit_digest: Some(consensus_commit_digest.to_string()),
            sub_dag_index,
            consensus_determined_version_assignments: Some(
                consensus_determined_version_assignments.into(),
            ),
            additional_state_digest: Some(additional_state_digest.to_string()),
        }
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
                    .map(|(tx_digest, assignments)| CanceledTransaction {
                        digest: Some(tx_digest.to_string()),
                        version_assignments: assignments
                            .into_iter()
                            .map(|(id, version)| VersionAssignment {
                                object_id: Some(id.to_canonical_string(true)),
                                start_version: None,
                                version: Some(version.value()),
                            })
                            .collect(),
                    })
                    .collect();
                1
            }
            A::CancelledTransactionsV2(canceled_transactions) => {
                message.canceled_transactions = canceled_transactions
                    .into_iter()
                    .map(|(tx_digest, assignments)| CanceledTransaction {
                        digest: Some(tx_digest.to_string()),
                        version_assignments: assignments
                            .into_iter()
                            .map(|((id, start_version), version)| VersionAssignment {
                                object_id: Some(id.to_canonical_string(true)),
                                start_version: Some(start_version.value()),
                                version: Some(version.value()),
                            })
                            .collect(),
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
        Self {
            objects: value.objects.into_iter().map(Into::into).collect(),
        }
    }
}

//
// RandomnessStateUpdate
//

impl From<crate::transaction::RandomnessStateUpdate> for RandomnessStateUpdate {
    fn from(value: crate::transaction::RandomnessStateUpdate) -> Self {
        Self {
            epoch: Some(value.epoch),
            randomness_round: Some(value.randomness_round.0),
            random_bytes: Some(value.random_bytes.into()),
            randomness_object_initial_shared_version: Some(
                value.randomness_obj_initial_shared_version.value(),
            ),
        }
    }
}

//
// AuthenticatorStateUpdate
//

impl From<crate::transaction::AuthenticatorStateUpdate> for AuthenticatorStateUpdate {
    fn from(value: crate::transaction::AuthenticatorStateUpdate) -> Self {
        Self {
            epoch: Some(value.epoch),
            round: Some(value.round),
            new_active_jwks: value.new_active_jwks.into_iter().map(Into::into).collect(),
            authenticator_object_initial_shared_version: Some(
                value.authenticator_obj_initial_shared_version.value(),
            ),
        }
    }
}

//
// ActiveJwk
//

impl From<crate::authenticator_state::ActiveJwk> for ActiveJwk {
    fn from(value: crate::authenticator_state::ActiveJwk) -> Self {
        Self {
            id: Some(JwkId {
                iss: Some(value.jwk_id.iss),
                kid: Some(value.jwk_id.kid),
            }),
            jwk: Some(Jwk {
                kty: Some(value.jwk.kty),
                e: Some(value.jwk.e),
                n: Some(value.jwk.n),
                alg: Some(value.jwk.alg),
            }),
            epoch: Some(value.epoch),
        }
    }
}

//
// ChangeEpoch
//

impl From<crate::transaction::ChangeEpoch> for ChangeEpoch {
    fn from(value: crate::transaction::ChangeEpoch) -> Self {
        Self {
            epoch: Some(value.epoch),
            protocol_version: Some(value.protocol_version.as_u64()),
            storage_charge: Some(value.storage_charge),
            computation_charge: Some(value.computation_charge),
            storage_rebate: Some(value.storage_rebate),
            non_refundable_storage_fee: Some(value.non_refundable_storage_fee),
            epoch_start_timestamp: Some(sui_rpc::proto::timestamp_ms_to_proto(
                value.epoch_start_timestamp_ms,
            )),
            system_packages: value
                .system_packages
                .into_iter()
                .map(|(version, modules, dependencies)| SystemPackage {
                    version: Some(version.value()),
                    modules: modules.into_iter().map(Into::into).collect(),
                    dependencies: dependencies
                        .iter()
                        .map(|d| d.to_canonical_string(true))
                        .collect(),
                })
                .collect(),
        }
    }
}

//
// EndOfEpochTransactionkind
//

impl From<crate::transaction::EndOfEpochTransactionKind> for EndOfEpochTransactionKind {
    fn from(value: crate::transaction::EndOfEpochTransactionKind) -> Self {
        use crate::transaction::EndOfEpochTransactionKind as K;
        use end_of_epoch_transaction_kind::Kind;

        let kind = match value {
            K::ChangeEpoch(change_epoch) => Kind::ChangeEpoch(change_epoch.into()),
            K::AuthenticatorStateCreate => Kind::AuthenticatorStateCreate(()),
            K::AuthenticatorStateExpire(expire) => Kind::AuthenticatorStateExpire(expire.into()),
            K::RandomnessStateCreate => Kind::RandomnessStateCreate(()),
            K::DenyListStateCreate => Kind::DenyListStateCreate(()),
            K::BridgeStateCreate(chain_id) => Kind::BridgeStateCreate(chain_id.to_string()),
            K::BridgeCommitteeInit(bridge_object_version) => {
                Kind::BridgeCommitteeInit(bridge_object_version.value())
            }
            K::StoreExecutionTimeObservations(observations) => {
                Kind::ExecutionTimeObservations(observations.into())
            }
            K::AccumulatorRootCreate => Kind::AccumulatorRootCreate(()),
            // K::CoinRegistryCreate => Kind::CoinRegistryCreate(()),
        };

        Self { kind: Some(kind) }
    }
}

//
// AuthenticatorStateExpire
//

impl From<crate::transaction::AuthenticatorStateExpire> for AuthenticatorStateExpire {
    fn from(value: crate::transaction::AuthenticatorStateExpire) -> Self {
        Self {
            min_epoch: Some(value.min_epoch),
            authenticator_object_initial_shared_version: Some(
                value.authenticator_obj_initial_shared_version.value(),
            ),
        }
    }
}

// ExecutionTimeObservations

impl From<crate::transaction::StoredExecutionTimeObservations> for ExecutionTimeObservations {
    fn from(value: crate::transaction::StoredExecutionTimeObservations) -> Self {
        match value {
            crate::transaction::StoredExecutionTimeObservations::V1(vec) => Self {
                version: Some(1),
                observations: vec
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
                                message.move_entry_point = Some(MoveCall {
                                    package: Some(package.to_canonical_string(true)),
                                    module: Some(module),
                                    function: Some(function),
                                    type_arguments: type_arguments
                                        .into_iter()
                                        .map(|ty| ty.to_canonical_string(true))
                                        .collect(),
                                    arguments: Vec::new(),
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
                            .map(|(name, duration)| ValidatorExecutionTimeObservation {
                                validator: Some(name.0.to_vec().into()),
                                duration: Some(prost_types::Duration {
                                    seconds: duration.as_secs() as i64,
                                    nanos: duration.subsec_nanos() as i32,
                                }),
                            })
                            .collect();

                        message.set_kind(kind);
                        message
                    })
                    .collect(),
            },
        }
    }
}

//
// ProgrammableTransaction
//

impl From<crate::transaction::ProgrammableTransaction> for ProgrammableTransaction {
    fn from(value: crate::transaction::ProgrammableTransaction) -> Self {
        Self {
            inputs: value.inputs.into_iter().map(Into::into).collect(),
            commands: value.commands.into_iter().map(Into::into).collect(),
        }
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
                    mutable,
                } => {
                    message.object_id = Some(id.to_canonical_string(true));
                    message.version = Some(initial_shared_version.value());
                    message.mutable = Some(mutable);
                    InputKind::Shared
                }
                O::Receiving((id, version, digest)) => {
                    message.object_id = Some(id.to_canonical_string(true));
                    message.version = Some(version.value());
                    message.digest = Some(digest.to_string());
                    InputKind::Receiving
                }
            },
            //TODO
            I::BalanceWithdraw(_) => InputKind::Unknown,
        };

        message.set_kind(kind);
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
            C::TransferObjects(objects, address) => Command::TransferObjects(TransferObjects {
                objects: objects.into_iter().map(Into::into).collect(),
                address: Some(address.into()),
            }),
            C::SplitCoins(coin, amounts) => Command::SplitCoins(SplitCoins {
                coin: Some(coin.into()),
                amounts: amounts.into_iter().map(Into::into).collect(),
            }),
            C::MergeCoins(coin, coins_to_merge) => Command::MergeCoins(MergeCoins {
                coin: Some(coin.into()),
                coins_to_merge: coins_to_merge.into_iter().map(Into::into).collect(),
            }),
            C::Publish(modules, dependencies) => Command::Publish(Publish {
                modules: modules.into_iter().map(Into::into).collect(),
                dependencies: dependencies
                    .iter()
                    .map(|d| d.to_canonical_string(true))
                    .collect(),
            }),
            C::MakeMoveVec(element_type, elements) => Command::MakeMoveVector(MakeMoveVector {
                element_type: element_type.map(|t| t.to_canonical_string(true)),
                elements: elements.into_iter().map(Into::into).collect(),
            }),
            C::Upgrade(modules, dependencies, package, ticket) => Command::Upgrade(Upgrade {
                modules: modules.into_iter().map(Into::into).collect(),
                dependencies: dependencies
                    .iter()
                    .map(|d| d.to_canonical_string(true))
                    .collect(),
                package: Some(package.to_canonical_string(true)),
                ticket: Some(ticket.into()),
            }),
        };

        Self {
            command: Some(command),
        }
    }
}

//
// MoveCall
//

impl From<crate::transaction::ProgrammableMoveCall> for MoveCall {
    fn from(value: crate::transaction::ProgrammableMoveCall) -> Self {
        Self {
            package: Some(value.package.to_canonical_string(true)),
            module: Some(value.module.to_string()),
            function: Some(value.function.to_string()),
            type_arguments: value
                .type_arguments
                .iter()
                .map(|t| t.to_canonical_string(true))
                .collect(),
            arguments: value.arguments.into_iter().map(Into::into).collect(),
        }
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
            || mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name)
            || mask.contains(Self::GAS_OBJECT_FIELD.name)
        {
            let mut changed_objects = Vec::new();
            let mut unchanged_shared_objects = Vec::new();

            for ((id, version, digest), owner) in value.created() {
                let change = ChangedObject {
                    object_id: Some(id.to_canonical_string(true)),
                    input_state: Some(changed_object::InputObjectState::DoesNotExist.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(changed_object::OutputObjectState::ObjectWrite.into()),
                    output_version: Some(version.value()),
                    output_digest: Some(digest.to_string()),
                    output_owner: Some(owner.clone().into()),
                    id_operation: Some(changed_object::IdOperation::Created.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for ((id, version, digest), owner) in value.mutated() {
                let change = ChangedObject {
                    object_id: Some(id.to_canonical_string(true)),
                    input_state: Some(changed_object::InputObjectState::Exists.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(changed_object::OutputObjectState::ObjectWrite.into()),
                    output_version: Some(version.value()),
                    output_digest: Some(digest.to_string()),
                    output_owner: Some(owner.clone().into()),
                    id_operation: Some(changed_object::IdOperation::None.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for ((id, version, digest), owner) in value.unwrapped() {
                let change = ChangedObject {
                    object_id: Some(id.to_canonical_string(true)),
                    input_state: Some(changed_object::InputObjectState::DoesNotExist.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(changed_object::OutputObjectState::ObjectWrite.into()),
                    output_version: Some(version.value()),
                    output_digest: Some(digest.to_string()),
                    output_owner: Some(owner.clone().into()),
                    id_operation: Some(changed_object::IdOperation::None.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for (id, version, digest) in value.deleted() {
                let change = ChangedObject {
                    object_id: Some(id.to_canonical_string(true)),
                    input_state: Some(changed_object::InputObjectState::Exists.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(changed_object::OutputObjectState::DoesNotExist.into()),
                    output_version: Some(version.value()),
                    output_digest: Some(digest.to_string()),
                    output_owner: None,
                    id_operation: Some(changed_object::IdOperation::Deleted.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for (id, version, digest) in value.unwrapped_then_deleted() {
                let change = ChangedObject {
                    object_id: Some(id.to_canonical_string(true)),
                    input_state: Some(changed_object::InputObjectState::DoesNotExist.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(changed_object::OutputObjectState::DoesNotExist.into()),
                    output_version: Some(version.value()),
                    output_digest: Some(digest.to_string()),
                    output_owner: None,
                    id_operation: Some(changed_object::IdOperation::Deleted.into()),
                    object_type: None,
                };

                changed_objects.push(change);
            }

            for (id, version, digest) in value.wrapped() {
                let change = ChangedObject {
                    object_id: Some(id.to_canonical_string(true)),
                    input_state: Some(changed_object::InputObjectState::Exists.into()),
                    input_version: None,
                    input_digest: None,
                    input_owner: None,
                    output_state: Some(changed_object::OutputObjectState::DoesNotExist.into()),
                    output_version: Some(version.value()),
                    output_digest: Some(digest.to_string()),
                    output_owner: None,
                    id_operation: Some(changed_object::IdOperation::Deleted.into()),
                    object_type: None,
                };

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
                    let unchanged_shared_object = UnchangedSharedObject {
                        kind: Some(
                            unchanged_shared_object::UnchangedSharedObjectKind::ReadOnlyRoot.into(),
                        ),
                        object_id: Some(object_id),
                        version: Some(version),
                        digest: Some(digest),
                        object_type: None,
                    };

                    unchanged_shared_objects.push(unchanged_shared_object);
                }
            }

            if mask.contains(Self::GAS_OBJECT_FIELD.name) {
                let gas_object_id = value.gas_object().0 .0.to_canonical_string(true);
                self.gas_object = changed_objects
                    .iter()
                    .find(|object| object.object_id() == gas_object_id)
                    .cloned();
            }

            if mask.contains(Self::CHANGED_OBJECTS_FIELD.name) {
                self.changed_objects = changed_objects;
            }

            if mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name) {
                self.unchanged_shared_objects = unchanged_shared_objects;
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
            unchanged_shared_objects,
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

        if mask.contains(Self::UNCHANGED_SHARED_OBJECTS_FIELD.name) {
            self.unchanged_shared_objects = unchanged_shared_objects
                .clone()
                .into_iter()
                .map(|(id, unchanged)| {
                    let mut message = UnchangedSharedObject::from(unchanged);
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
            //TODO
            ObjectOut::AccumulatorWriteV1(_) => OutputObjectState::Unknown,
        };
        message.set_output_state(output_state);

        message.set_id_operation(value.id_operation.into());
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
// UnchangedSharedObject
//

impl From<crate::effects::UnchangedSharedKind> for UnchangedSharedObject {
    fn from(value: crate::effects::UnchangedSharedKind) -> Self {
        use crate::effects::UnchangedSharedKind as K;
        use unchanged_shared_object::UnchangedSharedObjectKind;

        let mut message = Self::default();

        let kind = match value {
            K::ReadOnlyRoot((version, digest)) => {
                message.version = Some(version.value());
                message.digest = Some(digest.to_string());
                UnchangedSharedObjectKind::ReadOnlyRoot
            }
            K::MutateConsensusStreamEnded(version) => {
                message.version = Some(version.value());
                UnchangedSharedObjectKind::MutateDeleted
            }
            K::ReadConsensusStreamEnded(version) => {
                message.version = Some(version.value());
                UnchangedSharedObjectKind::ReadDeleted
            }
            K::Cancelled(version) => {
                message.version = Some(version.value());
                UnchangedSharedObjectKind::Canceled
            }
            K::PerEpochConfig => UnchangedSharedObjectKind::PerEpochConfig,
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
        }
    }
}
