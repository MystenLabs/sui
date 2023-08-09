// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context};
use camino::Utf8Path;
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::KeyPair;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use sui_config::genesis::{
    Genesis, GenesisCeremonyParameters, GenesisChainParameters, TokenDistributionSchedule,
    UnsignedGenesis,
};
use sui_execution::{self, Executor};
use sui_framework::BuiltInFramework;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::{
    ExecutionDigests, ObjectID, SequenceNumber, SuiAddress, TransactionDigest, TxContext,
};
use sui_types::committee::Committee;
use sui_types::crypto::{
    AuthorityKeyPair, AuthorityPublicKeyBytes, AuthoritySignInfo, AuthoritySignInfoTrait,
    AuthoritySignature, DefaultHash, SuiAuthoritySignature,
};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::epoch_data::EpochData;
use sui_types::gas::GasCharger;
use sui_types::gas_coin::GasCoin;
use sui_types::governance::StakedSui;
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary,
};
use sui_types::metrics::LimitsMetrics;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemState, SuiSystemStateTrait};
use sui_types::temporary_store::{InnerTemporaryStore, TemporaryStore};
use sui_types::transaction::{CallArg, Command, InputObjectKind, InputObjects, Transaction};
use sui_types::{SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS};
use tracing::trace;
use validator_info::{GenesisValidatorInfo, GenesisValidatorMetadata, ValidatorInfo};

pub mod validator_info;

const GENESIS_BUILDER_COMMITTEE_DIR: &str = "committee";
const GENESIS_BUILDER_PARAMETERS_FILE: &str = "parameters";
const GENESIS_BUILDER_TOKEN_DISTRIBUTION_SCHEDULE_FILE: &str = "token-distribution-schedule";
const GENESIS_BUILDER_SIGNATURE_DIR: &str = "signatures";
const GENESIS_BUILDER_UNSIGNED_GENESIS_FILE: &str = "unsigned-genesis";

pub struct Builder {
    parameters: GenesisCeremonyParameters,
    token_distribution_schedule: Option<TokenDistributionSchedule>,
    objects: BTreeMap<ObjectID, Object>,
    validators: BTreeMap<AuthorityPublicKeyBytes, GenesisValidatorInfo>,
    // Validator signatures over checkpoint
    signatures: BTreeMap<AuthorityPublicKeyBytes, AuthoritySignInfo>,
    built_genesis: Option<UnsignedGenesis>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            parameters: Default::default(),
            token_distribution_schedule: None,
            objects: Default::default(),
            validators: Default::default(),
            signatures: Default::default(),
            built_genesis: None,
        }
    }

    pub fn with_parameters(mut self, parameters: GenesisCeremonyParameters) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn with_token_distribution_schedule(
        mut self,
        token_distribution_schedule: TokenDistributionSchedule,
    ) -> Self {
        self.token_distribution_schedule = Some(token_distribution_schedule);
        self
    }

    pub fn with_protocol_version(mut self, v: ProtocolVersion) -> Self {
        self.parameters.protocol_version = v;
        self
    }

    pub fn add_object(mut self, object: Object) -> Self {
        self.objects.insert(object.id(), object);
        self
    }

    pub fn add_objects(mut self, objects: Vec<Object>) -> Self {
        for object in objects {
            self.objects.insert(object.id(), object);
        }
        self
    }

    pub fn add_validator(
        mut self,
        validator: ValidatorInfo,
        proof_of_possession: AuthoritySignature,
    ) -> Self {
        self.validators.insert(
            validator.protocol_key(),
            GenesisValidatorInfo {
                info: validator,
                proof_of_possession,
            },
        );
        self
    }

    pub fn validators(&self) -> &BTreeMap<AuthorityPublicKeyBytes, GenesisValidatorInfo> {
        &self.validators
    }

    pub fn add_validator_signature(mut self, keypair: &AuthorityKeyPair) -> Self {
        let UnsignedGenesis { checkpoint, .. } = self.build_unsigned_genesis_checkpoint();

        let name = keypair.public().into();
        assert!(
            self.validators.contains_key(&name),
            "provided keypair does not correspond to a validator in the validator set"
        );
        let checkpoint_signature = {
            let intent_msg = IntentMessage::new(
                Intent::sui_app(IntentScope::CheckpointSummary),
                checkpoint.clone(),
            );
            let signature = AuthoritySignature::new_secure(&intent_msg, &checkpoint.epoch, keypair);
            AuthoritySignInfo {
                epoch: checkpoint.epoch,
                authority: name,
                signature,
            }
        };

        self.signatures.insert(name, checkpoint_signature);

        self
    }

    pub fn unsigned_genesis_checkpoint(&self) -> Option<UnsignedGenesis> {
        self.built_genesis.clone()
    }

    pub fn build_unsigned_genesis_checkpoint(&mut self) -> UnsignedGenesis {
        if let Some(built_genesis) = &self.built_genesis {
            return built_genesis.clone();
        }

        // Verify that all input data is valid
        self.validate().unwrap();

        let objects = self.objects.clone().into_values().collect::<Vec<_>>();
        let validators = self.validators.clone().into_values().collect::<Vec<_>>();

        let token_distribution_schedule =
            if let Some(token_distribution_schedule) = &self.token_distribution_schedule {
                token_distribution_schedule.clone()
            } else {
                TokenDistributionSchedule::new_for_validators_with_default_allocation(
                    validators.iter().map(|v| v.info.sui_address()),
                )
            };

        self.built_genesis = Some(build_unsigned_genesis_data(
            &self.parameters,
            &token_distribution_schedule,
            &validators,
            &objects,
        ));

        self.token_distribution_schedule = Some(token_distribution_schedule);

        self.built_genesis.clone().unwrap()
    }

    fn committee(objects: &[Object]) -> Committee {
        let sui_system_object =
            get_sui_system_state(&objects).expect("Sui System State object must always exist");
        sui_system_object.get_current_epoch_committee().committee
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        self.parameters.protocol_version
    }

    pub fn build(mut self) -> Genesis {
        let UnsignedGenesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            events,
            objects,
        } = self.build_unsigned_genesis_checkpoint();

        let committee = Self::committee(&objects);

        let checkpoint = {
            let signatures = self.signatures.clone().into_values().collect();

            CertifiedCheckpointSummary::new(checkpoint, signatures, &committee).unwrap()
        };

        let genesis = Genesis::new(
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            events,
            objects,
        );

        // Verify that all on-chain state was properly created
        self.validate().unwrap();

        genesis
    }

    /// Validates the entire state of the build, no matter what the internal state is (input
    /// collection phase or output phase)
    pub fn validate(&self) -> anyhow::Result<(), anyhow::Error> {
        self.validate_inputs()?;
        self.validate_output();
        Ok(())
    }

    /// Runs through validation checks on the input values present in the builder
    fn validate_inputs(&self) -> anyhow::Result<(), anyhow::Error> {
        if !self.parameters.allow_insertion_of_extra_objects && !self.objects.is_empty() {
            bail!("extra objects are disallowed");
        }

        for validator in self.validators.values() {
            validator.validate().with_context(|| {
                format!(
                    "metadata for validator {} is invalid",
                    validator.info.name()
                )
            })?;
        }

        if let Some(token_distribution_schedule) = &self.token_distribution_schedule {
            token_distribution_schedule.validate();
            token_distribution_schedule.check_all_stake_operations_are_for_valid_validators(
                self.validators.values().map(|v| v.info.sui_address()),
            );
        }

        Ok(())
    }

    /// Runs through validation checks on the generated output (the initial chain state) based on
    /// the input values present in the builder
    fn validate_output(&self) {
        // If genesis hasn't been built yet, just early return as there is nothing to validate yet
        let Some(unsigned_genesis) = self.unsigned_genesis_checkpoint() else {
            return;
        };

        let GenesisChainParameters {
            protocol_version,
            chain_start_timestamp_ms,
            epoch_duration_ms,
            stake_subsidy_start_epoch,
            stake_subsidy_initial_distribution_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
        } = self.parameters.to_genesis_chain_parameters();

        // In non-testing code, genesis type must always be V1.
        let system_state = match unsigned_genesis.sui_system_object() {
            SuiSystemState::V1(inner) => inner,
            SuiSystemState::V2(_) => unreachable!(),
            #[cfg(msim)]
            _ => {
                // Types other than V1 used in simtests do not need to be validated.
                return;
            }
        };

        assert_eq!(
            self.validators.len(),
            system_state.validators.active_validators.len()
        );
        let mut address_to_pool_id = BTreeMap::new();
        for (validator, onchain_validator) in self
            .validators
            .values()
            .zip(system_state.validators.active_validators.iter())
        {
            let metadata = onchain_validator.verified_metadata();

            // Validators should not have duplicate addresses so the result of insertion should be None.
            assert!(address_to_pool_id
                .insert(metadata.sui_address, onchain_validator.staking_pool.id)
                .is_none());
            assert_eq!(validator.info.sui_address(), metadata.sui_address);
            assert_eq!(validator.info.protocol_key(), metadata.sui_pubkey_bytes());
            assert_eq!(validator.info.network_key, metadata.network_pubkey);
            assert_eq!(validator.info.worker_key, metadata.worker_pubkey);
            assert_eq!(
                validator.proof_of_possession.as_ref().to_vec(),
                metadata.proof_of_possession_bytes
            );
            assert_eq!(validator.info.name(), &metadata.name);
            assert_eq!(validator.info.description, metadata.description);
            assert_eq!(validator.info.image_url, metadata.image_url);
            assert_eq!(validator.info.project_url, metadata.project_url);
            assert_eq!(validator.info.network_address(), &metadata.net_address);
            assert_eq!(validator.info.p2p_address, metadata.p2p_address);
            assert_eq!(
                validator.info.narwhal_primary_address,
                metadata.primary_address
            );
            assert_eq!(
                validator.info.narwhal_worker_address,
                metadata.worker_address
            );

            assert_eq!(validator.info.gas_price, onchain_validator.gas_price);
            assert_eq!(
                validator.info.commission_rate,
                onchain_validator.commission_rate
            );
        }

        assert_eq!(system_state.epoch, 0);
        assert_eq!(system_state.protocol_version, protocol_version);
        assert_eq!(system_state.storage_fund.non_refundable_balance.value(), 0);
        assert_eq!(
            system_state
                .storage_fund
                .total_object_storage_rebates
                .value(),
            0
        );

        assert_eq!(system_state.parameters.epoch_duration_ms, epoch_duration_ms);
        assert_eq!(
            system_state.parameters.stake_subsidy_start_epoch,
            stake_subsidy_start_epoch,
        );
        assert_eq!(
            system_state.parameters.max_validator_count,
            max_validator_count,
        );
        assert_eq!(
            system_state.parameters.min_validator_joining_stake,
            min_validator_joining_stake,
        );
        assert_eq!(
            system_state.parameters.validator_low_stake_threshold,
            validator_low_stake_threshold,
        );
        assert_eq!(
            system_state.parameters.validator_very_low_stake_threshold,
            validator_very_low_stake_threshold,
        );
        assert_eq!(
            system_state.parameters.validator_low_stake_grace_period,
            validator_low_stake_grace_period,
        );

        assert_eq!(system_state.stake_subsidy.distribution_counter, 0);
        assert_eq!(
            system_state.stake_subsidy.current_distribution_amount,
            stake_subsidy_initial_distribution_amount,
        );
        assert_eq!(
            system_state.stake_subsidy.stake_subsidy_period_length,
            stake_subsidy_period_length,
        );
        assert_eq!(
            system_state.stake_subsidy.stake_subsidy_decrease_rate,
            stake_subsidy_decrease_rate,
        );

        assert!(!system_state.safe_mode);
        assert_eq!(
            system_state.epoch_start_timestamp_ms,
            chain_start_timestamp_ms,
        );
        assert_eq!(system_state.validators.pending_removals.len(), 0);
        assert_eq!(
            system_state
                .validators
                .pending_active_validators
                .contents
                .size,
            0
        );
        assert_eq!(system_state.validators.inactive_validators.size, 0);
        assert_eq!(system_state.validators.validator_candidates.size, 0);

        // Check distribution is correct
        let token_distribution_schedule = self.token_distribution_schedule.clone().unwrap();
        assert_eq!(
            system_state.stake_subsidy.balance.value(),
            token_distribution_schedule.stake_subsidy_fund_mist
        );

        let mut gas_objects: BTreeMap<ObjectID, (&Object, GasCoin)> = unsigned_genesis
            .objects()
            .iter()
            .filter_map(|o| GasCoin::try_from(o).ok().map(|g| (o.id(), (o, g))))
            .collect();
        let mut staked_sui_objects: BTreeMap<ObjectID, (&Object, StakedSui)> = unsigned_genesis
            .objects()
            .iter()
            .filter_map(|o| StakedSui::try_from(o).ok().map(|s| (o.id(), (o, s))))
            .collect();

        for allocation in token_distribution_schedule.allocations {
            if let Some(staked_with_validator) = allocation.staked_with_validator {
                let staking_pool_id = *address_to_pool_id
                    .get(&staked_with_validator)
                    .expect("staking pool should exist");
                let staked_sui_object_id = staked_sui_objects
                    .iter()
                    .find(|(_k, (o, s))| {
                        let Owner::AddressOwner(owner) = &o.owner else {
                        panic!("gas object owner must be address owner");
                    };
                        *owner == allocation.recipient_address
                            && s.principal() == allocation.amount_mist
                            && s.pool_id() == staking_pool_id
                    })
                    .map(|(k, _)| *k)
                    .expect("all allocations should be present");
                let staked_sui_object = staked_sui_objects.remove(&staked_sui_object_id).unwrap();
                assert_eq!(
                    staked_sui_object.0.owner,
                    Owner::AddressOwner(allocation.recipient_address)
                );
                assert_eq!(staked_sui_object.1.principal(), allocation.amount_mist);
                assert_eq!(staked_sui_object.1.pool_id(), staking_pool_id);
                assert_eq!(staked_sui_object.1.activation_epoch(), 0);
            } else {
                let gas_object_id = gas_objects
                    .iter()
                    .find(|(_k, (o, g))| {
                        if let Owner::AddressOwner(owner) = &o.owner {
                            *owner == allocation.recipient_address
                                && g.value() == allocation.amount_mist
                        } else {
                            false
                        }
                    })
                    .map(|(k, _)| *k)
                    .expect("all allocations should be present");
                let gas_object = gas_objects.remove(&gas_object_id).unwrap();
                assert_eq!(
                    gas_object.0.owner,
                    Owner::AddressOwner(allocation.recipient_address)
                );
                assert_eq!(gas_object.1.value(), allocation.amount_mist,);
            }
        }

        // All Gas and staked objects should be accounted for
        if !self.parameters.allow_insertion_of_extra_objects {
            assert!(gas_objects.is_empty());
            assert!(staked_sui_objects.is_empty());
        }

        let committee = system_state.get_current_epoch_committee().committee;
        for signature in self.signatures.values() {
            if self.validators.get(&signature.authority).is_none() {
                panic!("found signature for unknown validator: {:#?}", signature);
            }

            signature
                .verify_secure(
                    unsigned_genesis.checkpoint(),
                    Intent::sui_app(IntentScope::CheckpointSummary),
                    &committee,
                )
                .expect("signature should be valid");
        }
    }

    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self, anyhow::Error> {
        let path = path.as_ref();
        let path: &Utf8Path = path.try_into()?;
        trace!("Reading Genesis Builder from {}", path);

        if !path.is_dir() {
            bail!("path must be a directory");
        }

        // Load parameters
        let parameters_file = path.join(GENESIS_BUILDER_PARAMETERS_FILE);
        let parameters = serde_yaml::from_slice(
            &fs::read(parameters_file).context("unable to read genesis parameters file")?,
        )
        .context("unable to deserialize genesis parameters")?;

        let token_distribution_schedule_file =
            path.join(GENESIS_BUILDER_TOKEN_DISTRIBUTION_SCHEDULE_FILE);
        let token_distribution_schedule = if token_distribution_schedule_file.exists() {
            Some(TokenDistributionSchedule::from_csv(fs::File::open(
                token_distribution_schedule_file,
            )?)?)
        } else {
            None
        };

        // Load validator infos
        let mut committee = BTreeMap::new();
        for entry in path.join(GENESIS_BUILDER_COMMITTEE_DIR).read_dir_utf8()? {
            let entry = entry?;
            if entry.file_name().starts_with('.') {
                continue;
            }

            let path = entry.path();
            let validator_info_bytes = fs::read(path)?;
            let validator_info: GenesisValidatorInfo =
                serde_yaml::from_slice(&validator_info_bytes)
                    .with_context(|| format!("unable to load validator info for {path}"))?;
            committee.insert(validator_info.info.protocol_key(), validator_info);
        }

        // Load Signatures
        let mut signatures = BTreeMap::new();
        for entry in path.join(GENESIS_BUILDER_SIGNATURE_DIR).read_dir_utf8()? {
            let entry = entry?;
            if entry.file_name().starts_with('.') {
                continue;
            }

            let path = entry.path();
            let signature_bytes = fs::read(path)?;
            let sigs: AuthoritySignInfo = bcs::from_bytes(&signature_bytes)
                .with_context(|| format!("unable to load validator signatrue for {path}"))?;
            signatures.insert(sigs.authority, sigs);
        }

        let mut builder = Self {
            parameters,
            token_distribution_schedule,
            objects: Default::default(),
            validators: committee,
            signatures,
            built_genesis: None, // Leave this as none, will build and compare below
        };

        let unsigned_genesis_file = path.join(GENESIS_BUILDER_UNSIGNED_GENESIS_FILE);
        if unsigned_genesis_file.exists() {
            let unsigned_genesis_bytes = fs::read(unsigned_genesis_file)?;
            let loaded_genesis: UnsignedGenesis = bcs::from_bytes(&unsigned_genesis_bytes)?;

            // If we have a built genesis, then we must have a token_distribution_schedule present
            // as well.
            assert!(
                builder.token_distribution_schedule.is_some(),
                "If a built genesis is present, then there must also be a token-distribution-schedule present"
            );

            // Verify loaded genesis matches one build from the constituent parts
            let built = builder.build_unsigned_genesis_checkpoint();
            loaded_genesis.checkpoint_contents.digest(); // cache digest before compare
            assert_eq!(
                built, loaded_genesis,
                "loaded genesis does not match built genesis"
            );

            // Just to double check that its set after building above
            assert!(builder.unsigned_genesis_checkpoint().is_some());
        }

        Ok(builder)
    }

    pub fn save<P: AsRef<Path>>(self, path: P) -> anyhow::Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis Builder to {}", path.display());

        fs::create_dir_all(path)?;

        // Write parameters
        let parameters_file = path.join(GENESIS_BUILDER_PARAMETERS_FILE);
        fs::write(parameters_file, serde_yaml::to_string(&self.parameters)?)?;

        if let Some(token_distribution_schedule) = &self.token_distribution_schedule {
            token_distribution_schedule.to_csv(fs::File::create(
                path.join(GENESIS_BUILDER_TOKEN_DISTRIBUTION_SCHEDULE_FILE),
            )?)?;
        }

        // Write Signatures
        let signature_dir = path.join(GENESIS_BUILDER_SIGNATURE_DIR);
        std::fs::create_dir_all(&signature_dir)?;
        for (pubkey, sigs) in self.signatures {
            let sig_bytes = bcs::to_bytes(&sigs)?;
            let name = self.validators.get(&pubkey).unwrap().info.name();
            fs::write(signature_dir.join(name), sig_bytes)?;
        }

        // Write validator infos
        let committee_dir = path.join(GENESIS_BUILDER_COMMITTEE_DIR);
        fs::create_dir_all(&committee_dir)?;

        for (_pubkey, validator) in self.validators {
            let validator_info_bytes = serde_yaml::to_string(&validator)?;
            fs::write(
                committee_dir.join(validator.info.name()),
                validator_info_bytes,
            )?;
        }

        if let Some(genesis) = &self.built_genesis {
            let genesis_bytes = bcs::to_bytes(&genesis)?;
            fs::write(
                path.join(GENESIS_BUILDER_UNSIGNED_GENESIS_FILE),
                genesis_bytes,
            )?;
        }

        Ok(())
    }
}

// Create a Genesis Txn Context to be used when generating genesis objects by hashing all of the
// inputs into genesis ans using that as our "Txn Digest". This is done to ensure that coin objects
// created between chains are unique
fn create_genesis_context(
    epoch_data: &EpochData,
    genesis_chain_parameters: &GenesisChainParameters,
    genesis_validators: &[GenesisValidatorMetadata],
    token_distribution_schedule: &TokenDistributionSchedule,
) -> TxContext {
    let mut hasher = DefaultHash::default();
    hasher.update(b"sui-genesis");
    hasher.update(&bcs::to_bytes(genesis_chain_parameters).unwrap());
    hasher.update(&bcs::to_bytes(genesis_validators).unwrap());
    hasher.update(&bcs::to_bytes(token_distribution_schedule).unwrap());
    for system_package in BuiltInFramework::iter_system_packages() {
        hasher.update(&bcs::to_bytes(system_package.bytes()).unwrap());
    }

    let hash = hasher.finalize();
    let genesis_transaction_digest = TransactionDigest::new(hash.into());

    TxContext::new(
        &SuiAddress::default(),
        &genesis_transaction_digest,
        epoch_data,
    )
}

fn build_unsigned_genesis_data(
    parameters: &GenesisCeremonyParameters,
    token_distribution_schedule: &TokenDistributionSchedule,
    validators: &[GenesisValidatorInfo],
    objects: &[Object],
) -> UnsignedGenesis {
    if !parameters.allow_insertion_of_extra_objects && !objects.is_empty() {
        panic!("insertion of extra objects at genesis time is prohibited due to 'allow_insertion_of_extra_objects' parameter");
    }

    let genesis_chain_parameters = parameters.to_genesis_chain_parameters();
    let genesis_validators = validators
        .iter()
        .cloned()
        .map(GenesisValidatorMetadata::from)
        .collect::<Vec<_>>();

    token_distribution_schedule.validate();
    token_distribution_schedule.check_all_stake_operations_are_for_valid_validators(
        genesis_validators.iter().map(|v| v.sui_address),
    );

    let epoch_data = EpochData::new_genesis(genesis_chain_parameters.chain_start_timestamp_ms);

    let mut genesis_ctx = create_genesis_context(
        &epoch_data,
        &genesis_chain_parameters,
        &genesis_validators,
        token_distribution_schedule,
    );

    // Use a throwaway metrics registry for genesis transaction execution.
    let registry = prometheus::Registry::new();
    let metrics = Arc::new(LimitsMetrics::new(&registry));

    let objects = create_genesis_objects(
        &mut genesis_ctx,
        objects,
        &genesis_validators,
        &genesis_chain_parameters,
        token_distribution_schedule,
        metrics.clone(),
    );

    // We have a circular dependency here. Protocol config depends on chain ID, which
    // depends on genesis checkpoint (digest), which depends on genesis transaction, which
    // depends on protocol config.
    // However since we know there are no chain specific protocol config options in genesis,
    // we use Chain::Unknown here.
    let protocol_config =
        ProtocolConfig::get_for_version(parameters.protocol_version, Chain::Unknown);

    let (genesis_transaction, genesis_effects, genesis_events, objects) =
        create_genesis_transaction(objects, &protocol_config, metrics, &epoch_data);
    let (checkpoint, checkpoint_contents) =
        create_genesis_checkpoint(parameters, &genesis_transaction, &genesis_effects);

    UnsignedGenesis {
        checkpoint,
        checkpoint_contents,
        transaction: genesis_transaction,
        effects: genesis_effects,
        events: genesis_events,
        objects,
    }
}

fn create_genesis_checkpoint(
    parameters: &GenesisCeremonyParameters,
    transaction: &Transaction,
    effects: &TransactionEffects,
) -> (CheckpointSummary, CheckpointContents) {
    let execution_digests = ExecutionDigests {
        transaction: *transaction.digest(),
        effects: effects.digest(),
    };
    let contents = CheckpointContents::new_with_causally_ordered_transactions([execution_digests]);
    let checkpoint = CheckpointSummary {
        epoch: 0,
        sequence_number: 0,
        network_total_transactions: contents.size().try_into().unwrap(),
        content_digest: *contents.digest(),
        previous_digest: None,
        epoch_rolling_gas_cost_summary: Default::default(),
        end_of_epoch_data: None,
        timestamp_ms: parameters.chain_start_timestamp_ms,
        version_specific_data: Vec::new(),
        checkpoint_commitments: Default::default(),
    };

    (checkpoint, contents)
}

fn create_genesis_transaction(
    objects: Vec<Object>,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    epoch_data: &EpochData,
) -> (
    Transaction,
    TransactionEffects,
    TransactionEvents,
    Vec<Object>,
) {
    let genesis_transaction = {
        let genesis_objects = objects
            .into_iter()
            .map(|mut object| {
                if let Some(o) = object.data.try_as_move_mut() {
                    o.decrement_version_to(SequenceNumber::MIN);
                }

                if let Owner::Shared {
                    initial_shared_version,
                } = &mut object.owner
                {
                    *initial_shared_version = SequenceNumber::MIN;
                }

                sui_types::transaction::GenesisObject::RawObject {
                    data: object.data,
                    owner: object.owner,
                }
            })
            .collect();

        sui_types::transaction::VerifiedTransaction::new_genesis_transaction(genesis_objects)
            .into_inner()
    };

    let genesis_digest = *genesis_transaction.digest();
    // execute txn to effects
    let (effects, events, objects) = {
        let temporary_store = TemporaryStore::new(
            InMemoryStorage::new(Vec::new()),
            InputObjects::new(vec![]),
            genesis_digest,
            protocol_config,
        );

        let silent = true;
        let paranoid_checks = false;
        let executor = sui_execution::executor(protocol_config, paranoid_checks, silent)
            .expect("Creating an executor should not fail here");

        let expensive_checks = false;
        let certificate_deny_set = HashSet::new();
        let shared_object_refs = vec![];
        let transaction_data = &genesis_transaction.data().intent_message().value;
        let (kind, signer, _) = transaction_data.execution_parts();
        let transaction_dependencies = BTreeSet::new();
        let (inner_temp_store, effects, _execution_error) = executor
            .execute_transaction_to_effects(
                protocol_config,
                metrics,
                expensive_checks,
                &certificate_deny_set,
                &epoch_data.epoch_id(),
                epoch_data.epoch_start_timestamp(),
                temporary_store,
                shared_object_refs,
                &mut GasCharger::new_unmetered(genesis_digest),
                kind,
                signer,
                genesis_digest,
                transaction_dependencies,
            );
        assert!(inner_temp_store.objects.is_empty());
        assert!(inner_temp_store.mutable_inputs.is_empty());
        assert!(inner_temp_store.deleted.is_empty());

        let objects = inner_temp_store
            .written
            .into_iter()
            .map(|(_, (_, o, kind))| {
                assert_eq!(kind, sui_types::storage::WriteKind::Create);
                o
            })
            .collect();
        (effects, inner_temp_store.events, objects)
    };

    (genesis_transaction, effects, events, objects)
}

fn create_genesis_objects(
    genesis_ctx: &mut TxContext,
    input_objects: &[Object],
    validators: &[GenesisValidatorMetadata],
    parameters: &GenesisChainParameters,
    token_distribution_schedule: &TokenDistributionSchedule,
    metrics: Arc<LimitsMetrics>,
) -> Vec<Object> {
    let mut store = InMemoryStorage::new(Vec::new());
    // We don't know the chain ID here since we haven't yet created the genesis checkpoint.
    // However since we know there are no chain specific protool config options in genesis,
    // we use Chain::Unknown here.
    let protocol_config = ProtocolConfig::get_for_version(
        ProtocolVersion::new(parameters.protocol_version),
        Chain::Unknown,
    );

    let silent = true;
    // paranoid checks are a last line of defense for malicious code, no need to run them in genesis
    let paranoid_checks = false;
    let executor = sui_execution::executor(&protocol_config, paranoid_checks, silent)
        .expect("Creating an executor should not fail here");

    for system_package in BuiltInFramework::iter_system_packages() {
        process_package(
            &mut store,
            executor.as_ref(),
            genesis_ctx,
            &system_package.modules(),
            system_package.dependencies().to_vec(),
            &protocol_config,
            metrics.clone(),
        )
        .unwrap();
    }

    {
        let store = Arc::get_mut(&mut store).expect("only one reference to store");
        for object in input_objects {
            store.insert_object(object.to_owned());
        }
    }

    generate_genesis_system_object(
        &mut store,
        executor.as_ref(),
        validators,
        genesis_ctx,
        parameters,
        token_distribution_schedule,
        metrics,
    )
    .unwrap();

    let store = Arc::try_unwrap(store).expect("only one reference to store");
    store.into_inner().into_values().collect()
}

fn process_package(
    store: &mut Arc<InMemoryStorage>,
    executor: &dyn Executor,
    ctx: &mut TxContext,
    modules: &[CompiledModule],
    dependencies: Vec<ObjectID>,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> anyhow::Result<()> {
    let dependency_objects = store.get_objects(&dependencies);
    // When publishing genesis packages, since the std framework packages all have
    // non-zero addresses, [`Transaction::input_objects_in_compiled_modules`] will consider
    // them as dependencies even though they are not. Hence input_objects contain objects
    // that don't exist on-chain because they are yet to be published.
    #[cfg(debug_assertions)]
    {
        use move_core_types::account_address::AccountAddress;
        let to_be_published_addresses: HashSet<_> = modules
            .iter()
            .map(|module| *module.self_id().address())
            .collect();
        assert!(
            // An object either exists on-chain, or is one of the packages to be published.
            dependencies
                .iter()
                .zip(dependency_objects.iter())
                .all(|(dependency, obj_opt)| obj_opt.is_some()
                    || to_be_published_addresses.contains(&AccountAddress::from(*dependency)))
        );
    }
    let loaded_dependencies: Vec<_> = dependencies
        .iter()
        .zip(dependency_objects.into_iter())
        .filter_map(|(dependency, object)| {
            Some((
                InputObjectKind::MovePackage(*dependency),
                object?.to_owned(),
            ))
        })
        .collect();

    let genesis_digest = ctx.digest();
    let mut temporary_store = TemporaryStore::new(
        store.clone(),
        InputObjects::new(loaded_dependencies),
        genesis_digest,
        protocol_config,
    );
    let module_bytes = modules
        .iter()
        .map(|m| {
            let mut buf = vec![];
            m.serialize(&mut buf).unwrap();
            buf
        })
        .collect();
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        // executing in Genesis mode does not create an `UpgradeCap`.
        builder.command(Command::Publish(module_bytes, dependencies));
        builder.finish()
    };
    executor.update_genesis_state(
        protocol_config,
        metrics,
        &mut temporary_store,
        ctx,
        &mut GasCharger::new_unmetered(genesis_digest),
        pt,
    )?;

    let InnerTemporaryStore {
        written, deleted, ..
    } = temporary_store.into_inner();

    let store = Arc::get_mut(store).expect("only one reference to store");
    store.finish(written, deleted);

    Ok(())
}

pub fn generate_genesis_system_object(
    store: &mut Arc<InMemoryStorage>,
    executor: &dyn Executor,
    genesis_validators: &[GenesisValidatorMetadata],
    genesis_ctx: &mut TxContext,
    genesis_chain_parameters: &GenesisChainParameters,
    token_distribution_schedule: &TokenDistributionSchedule,
    metrics: Arc<LimitsMetrics>,
) -> anyhow::Result<()> {
    let genesis_digest = genesis_ctx.digest();
    // We don't know the chain ID here since we haven't yet created the genesis checkpoint.
    // However since we know there are no chain specific protocol config options in genesis,
    // we use Chain::Unknown here.
    let protocol_config = ProtocolConfig::get_for_version(
        ProtocolVersion::new(genesis_chain_parameters.protocol_version),
        sui_protocol_config::Chain::Unknown,
    );
    let mut temporary_store = TemporaryStore::new(
        store.clone(),
        InputObjects::new(vec![]),
        genesis_digest,
        &protocol_config,
    );

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        // Step 1: Create the SuiSystemState UID
        let sui_system_state_uid = builder.programmable_move_call(
            SUI_FRAMEWORK_ADDRESS.into(),
            ident_str!("object").to_owned(),
            ident_str!("sui_system_state").to_owned(),
            vec![],
            vec![],
        );

        // Step 2: Create and share the Clock.
        builder.move_call(
            SUI_FRAMEWORK_ADDRESS.into(),
            ident_str!("clock").to_owned(),
            ident_str!("create").to_owned(),
            vec![],
            vec![],
        )?;

        // Step 3: Mint the supply of SUI.
        let sui_supply = builder.programmable_move_call(
            SUI_FRAMEWORK_ADDRESS.into(),
            ident_str!("sui").to_owned(),
            ident_str!("new").to_owned(),
            vec![],
            vec![],
        );

        // Step 4: Run genesis.
        // The first argument is the system state uid we got from step 1 and the second one is the SUI supply we
        // got from step 3.
        let mut arguments = vec![sui_system_state_uid, sui_supply];
        let mut call_arg_arguments = vec![
            CallArg::Pure(bcs::to_bytes(&genesis_chain_parameters).unwrap()),
            CallArg::Pure(bcs::to_bytes(&genesis_validators).unwrap()),
            CallArg::Pure(bcs::to_bytes(&token_distribution_schedule).unwrap()),
        ]
        .into_iter()
        .map(|a| builder.input(a))
        .collect::<anyhow::Result<_, _>>()?;
        arguments.append(&mut call_arg_arguments);
        builder.programmable_move_call(
            SUI_SYSTEM_ADDRESS.into(),
            ident_str!("genesis").to_owned(),
            ident_str!("create").to_owned(),
            vec![],
            arguments,
        );
        builder.finish()
    };

    executor.update_genesis_state(
        &protocol_config,
        metrics,
        &mut temporary_store,
        genesis_ctx,
        &mut GasCharger::new_unmetered(genesis_digest),
        pt,
    )?;

    let InnerTemporaryStore {
        written, deleted, ..
    } = temporary_store.into_inner();

    let store = Arc::get_mut(store).expect("only one reference to store");
    store.finish(written, deleted);

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::validator_info::ValidatorInfo;
    use crate::Builder;
    use fastcrypto::traits::KeyPair;
    use sui_config::genesis::*;
    use sui_config::local_ip_utils;
    use sui_config::node::DEFAULT_COMMISSION_RATE;
    use sui_config::node::DEFAULT_VALIDATOR_GAS_PRICE;
    use sui_types::base_types::SuiAddress;
    use sui_types::crypto::{
        generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
        NetworkKeyPair,
    };

    #[test]
    fn allocation_csv() {
        let schedule = TokenDistributionSchedule::new_for_validators_with_default_allocation([
            SuiAddress::random_for_testing_only(),
            SuiAddress::random_for_testing_only(),
        ]);
        let mut output = Vec::new();

        schedule.to_csv(&mut output).unwrap();

        let parsed_schedule = TokenDistributionSchedule::from_csv(output.as_slice()).unwrap();

        assert_eq!(schedule, parsed_schedule);

        std::io::Write::write_all(&mut std::io::stdout(), &output).unwrap();
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn ceremony() {
        let dir = tempfile::TempDir::new().unwrap();

        let key: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let worker_key: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let account_key: AccountKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let network_key: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let validator = ValidatorInfo {
            name: "0".into(),
            protocol_key: key.public().into(),
            worker_key: worker_key.public().clone(),
            account_address: SuiAddress::from(account_key.public()),
            network_key: network_key.public().clone(),
            gas_price: DEFAULT_VALIDATOR_GAS_PRICE,
            commission_rate: DEFAULT_COMMISSION_RATE,
            network_address: local_ip_utils::new_local_tcp_address_for_testing(),
            p2p_address: local_ip_utils::new_local_udp_address_for_testing(),
            narwhal_primary_address: local_ip_utils::new_local_udp_address_for_testing(),
            narwhal_worker_address: local_ip_utils::new_local_udp_address_for_testing(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
        };
        let pop = generate_proof_of_possession(&key, account_key.public().into());
        let mut builder = Builder::new().add_validator(validator, pop);

        let genesis = builder.build_unsigned_genesis_checkpoint();
        for object in genesis.objects() {
            println!("ObjectID: {} Type: {:?}", object.id(), object.type_());
        }
        builder.save(dir.path()).unwrap();
        Builder::load(dir.path()).unwrap();
    }
}
