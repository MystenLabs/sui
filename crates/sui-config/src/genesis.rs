// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorInfo;
use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::KeyPair;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::sync::Arc;
use std::{fs, path::Path};
use sui_adapter::adapter::MoveVM;
use sui_adapter::{adapter, execution_mode, programmable_transactions};
use sui_framework::BuiltInFramework;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ExecutionDigests, TransactionDigest};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::clock::Clock;
use sui_types::committee::CommitteeWithNetworkMetadata;
use sui_types::crypto::{
    verify_proof_of_possession, AuthorityPublicKey, AuthoritySignInfoTrait, DefaultHash,
};
use sui_types::crypto::{
    AuthorityKeyPair, AuthorityPublicKeyBytes, AuthoritySignInfo, AuthoritySignature,
    SuiAuthoritySignature, ToFromBytes,
};
use sui_types::epoch_data::EpochData;
use sui_types::gas::SuiGasStatus;
use sui_types::gas_coin::{GasCoin, TOTAL_SUPPLY_MIST};
use sui_types::governance::StakedSui;
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::message_envelope::Message;
use sui_types::messages::{
    CallArg, Command, InputObjectKind, InputObjects, Transaction, TransactionEffects,
    TransactionEvents,
};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, VerifiedCheckpoint,
};
use sui_types::metrics::LimitsMetrics;
use sui_types::multiaddr::Multiaddr;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::{
    get_sui_system_state, get_sui_system_state_wrapper, SuiSystemState, SuiSystemStateTrait,
    SuiSystemStateWrapper, SuiValidatorGenesis,
};
use sui_types::temporary_store::{InnerTemporaryStore, TemporaryStore};
use sui_types::{
    base_types::TxContext,
    committee::{Committee, EpochId, ProtocolVersion},
    error::SuiResult,
    object::Object,
};
use sui_types::{SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS};
use tracing::trace;

const MAX_VALIDATOR_METADATA_LENGTH: usize = 256;

#[derive(Clone, Debug)]
pub struct Genesis {
    checkpoint: CertifiedCheckpointSummary,
    checkpoint_contents: CheckpointContents,
    transaction: Transaction,
    effects: TransactionEffects,
    events: TransactionEvents,
    objects: Vec<Object>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct UnsignedGenesis {
    pub checkpoint: CheckpointSummary,
    pub checkpoint_contents: CheckpointContents,
    pub transaction: Transaction,
    pub effects: TransactionEffects,
    pub events: TransactionEvents,
    pub objects: Vec<Object>,
}

// Hand implement PartialEq in order to get around the fact that AuthSigs don't impl Eq
impl PartialEq for Genesis {
    fn eq(&self, other: &Self) -> bool {
        self.checkpoint.data() == other.checkpoint.data()
            && {
                let this = self.checkpoint.auth_sig();
                let other = other.checkpoint.auth_sig();

                this.epoch == other.epoch
                    && this.signature.as_ref() == other.signature.as_ref()
                    && this.signers_map == other.signers_map
            }
            && self.checkpoint_contents == other.checkpoint_contents
            && self.transaction == other.transaction
            && self.effects == other.effects
            && self.objects == other.objects
    }
}

impl Eq for Genesis {}

impl Genesis {
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn object(&self, id: ObjectID) -> Option<Object> {
        self.objects.iter().find(|o| o.id() == id).cloned()
    }

    pub fn transaction(&self) -> &Transaction {
        &self.transaction
    }

    pub fn effects(&self) -> &TransactionEffects {
        &self.effects
    }
    pub fn events(&self) -> &TransactionEvents {
        &self.events
    }

    pub fn checkpoint(&self) -> VerifiedCheckpoint {
        self.checkpoint
            .clone()
            .verify(&self.committee().unwrap())
            .unwrap()
    }

    pub fn checkpoint_contents(&self) -> &CheckpointContents {
        &self.checkpoint_contents
    }

    pub fn epoch(&self) -> EpochId {
        0
    }

    pub fn validator_set_for_tooling(&self) -> Vec<SuiValidatorGenesis> {
        self.sui_system_object()
            .into_genesis_version_for_tooling()
            .validators
            .active_validators
    }

    pub fn committee_with_network(&self) -> CommitteeWithNetworkMetadata {
        self.sui_system_object().get_current_epoch_committee()
    }

    pub fn reference_gas_price(&self) -> u64 {
        self.sui_system_object().reference_gas_price()
    }

    // TODO: No need to return SuiResult.
    pub fn committee(&self) -> SuiResult<Committee> {
        Ok(self.committee_with_network().committee)
    }

    pub fn sui_system_wrapper_object(&self) -> SuiSystemStateWrapper {
        get_sui_system_state_wrapper(&self.objects())
            .expect("Sui System State Wrapper object must always exist")
    }

    pub fn sui_system_object(&self) -> SuiSystemState {
        get_sui_system_state(&self.objects()).expect("Sui System State object must always exist")
    }

    pub fn clock(&self) -> Clock {
        let clock = self
            .objects()
            .iter()
            .find(|o| o.id() == sui_types::SUI_CLOCK_OBJECT_ID)
            .expect("Clock must always exist")
            .data
            .try_as_move()
            .expect("Clock must be a Move object");
        bcs::from_bytes::<Clock>(clock.contents())
            .expect("Clock object deserialization cannot fail")
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let path = path.as_ref();
        trace!("Reading Genesis from {}", path.display());
        let bytes = fs::read(path)
            .with_context(|| format!("Unable to load Genesis from {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("Unable to parse Genesis from {}", path.display()))
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis to {}", path.display());
        let bytes = bcs::to_bytes(&self)?;
        fs::write(path, bytes)
            .with_context(|| format!("Unable to save Genesis to {}", path.display()))?;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(self).expect("failed to serialize genesis")
    }

    pub fn hash(&self) -> [u8; 32] {
        use std::io::Write;

        let mut digest = DefaultHash::default();
        digest.write_all(&self.to_bytes()).unwrap();
        let hash = digest.finalize();
        hash.into()
    }
}

impl Serialize for Genesis {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::Error;

        #[derive(Serialize)]
        struct RawGenesis<'a> {
            checkpoint: &'a CertifiedCheckpointSummary,
            checkpoint_contents: &'a CheckpointContents,
            transaction: &'a Transaction,
            effects: &'a TransactionEffects,
            events: &'a TransactionEvents,
            objects: &'a [Object],
        }

        let raw_genesis = RawGenesis {
            checkpoint: &self.checkpoint,
            checkpoint_contents: &self.checkpoint_contents,
            transaction: &self.transaction,
            effects: &self.effects,
            events: &self.events,
            objects: &self.objects,
        };

        let bytes = bcs::to_bytes(&raw_genesis).map_err(|e| Error::custom(e.to_string()))?;

        if serializer.is_human_readable() {
            let s = Base64::encode(&bytes);
            serializer.serialize_str(&s)
        } else {
            serializer.serialize_bytes(&bytes)
        }
    }
}

impl<'de> Deserialize<'de> for Genesis {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        struct RawGenesis {
            checkpoint: CertifiedCheckpointSummary,
            checkpoint_contents: CheckpointContents,
            transaction: Transaction,
            effects: TransactionEffects,
            events: TransactionEvents,
            objects: Vec<Object>,
        }

        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Base64::decode(&s).map_err(|e| Error::custom(e.to_string()))?
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            data
        };

        let RawGenesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            events,
            objects,
        } = bcs::from_bytes(&bytes).map_err(|e| Error::custom(e.to_string()))?;

        Ok(Genesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            events,
            objects,
        })
    }
}

impl UnsignedGenesis {
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn object(&self, id: ObjectID) -> Option<Object> {
        self.objects.iter().find(|o| o.id() == id).cloned()
    }

    pub fn transaction(&self) -> &Transaction {
        &self.transaction
    }

    pub fn effects(&self) -> &TransactionEffects {
        &self.effects
    }
    pub fn events(&self) -> &TransactionEvents {
        &self.events
    }

    pub fn checkpoint(&self) -> &CheckpointSummary {
        &self.checkpoint
    }

    pub fn checkpoint_contents(&self) -> &CheckpointContents {
        &self.checkpoint_contents
    }

    pub fn epoch(&self) -> EpochId {
        0
    }

    pub fn sui_system_wrapper_object(&self) -> SuiSystemStateWrapper {
        get_sui_system_state_wrapper(&self.objects())
            .expect("Sui System State Wrapper object must always exist")
    }

    pub fn sui_system_object(&self) -> SuiSystemState {
        get_sui_system_state(&self.objects()).expect("Sui System State object must always exist")
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisValidatorInfo {
    pub info: ValidatorInfo,
    pub proof_of_possession: AuthoritySignature,
}

impl GenesisValidatorInfo {
    fn validate(&self) -> Result<(), anyhow::Error> {
        if !self.info.name.is_ascii() {
            bail!("name must be ascii");
        }
        if self.info.name.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("name must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.description.is_ascii() {
            bail!("description must be ascii");
        }
        if self.info.description.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("description must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if self.info.image_url.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("image url must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if self.info.project_url.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("project url must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.network_address.to_string().is_ascii() {
            bail!("network address must be ascii");
        }
        if self.info.network_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("network address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.p2p_address.to_string().is_ascii() {
            bail!("p2p address must be ascii");
        }
        if self.info.p2p_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("p2p address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.narwhal_primary_address.to_string().is_ascii() {
            bail!("primary address must be ascii");
        }
        if self.info.narwhal_primary_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("primary address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if !self.info.narwhal_worker_address.to_string().is_ascii() {
            bail!("worker address must be ascii");
        }
        if self.info.narwhal_worker_address.len() > MAX_VALIDATOR_METADATA_LENGTH {
            bail!("worker address must be <= {MAX_VALIDATOR_METADATA_LENGTH} bytes long");
        }

        if let Err(e) = self.info.p2p_address.to_anemo_address() {
            bail!("p2p address must be valid anemo address: {e}");
        }
        if let Err(e) = self.info.narwhal_primary_address.to_anemo_address() {
            bail!("primary address must be valid anemo address: {e}");
        }
        if let Err(e) = self.info.narwhal_worker_address.to_anemo_address() {
            bail!("worker address must be valid anemo address: {e}");
        }

        if self.info.commission_rate > 10000 {
            bail!("commissions rate must be lower than 100%");
        }

        let protocol_pubkey = AuthorityPublicKey::from_bytes(self.info.protocol_key.as_ref())?;
        if let Err(e) = verify_proof_of_possession(
            &self.proof_of_possession,
            &protocol_pubkey,
            self.info.account_address,
        ) {
            bail!("proof of possession is incorrect: {e}");
        }

        Ok(())
    }
}

impl From<GenesisValidatorInfo> for GenesisValidatorMetadata {
    fn from(
        GenesisValidatorInfo {
            info,
            proof_of_possession,
        }: GenesisValidatorInfo,
    ) -> Self {
        Self {
            name: info.name,
            description: info.description,
            image_url: info.image_url,
            project_url: info.project_url,
            sui_address: info.account_address,
            gas_price: info.gas_price,
            commission_rate: info.commission_rate,
            protocol_public_key: info.protocol_key.as_bytes().to_vec(),
            proof_of_possession: proof_of_possession.as_ref().to_vec(),
            network_public_key: info.network_key.as_bytes().to_vec(),
            worker_public_key: info.worker_key.as_bytes().to_vec(),
            network_address: info.network_address,
            p2p_address: info.p2p_address,
            primary_address: info.narwhal_primary_address,
            worker_address: info.narwhal_worker_address,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GenesisValidatorMetadata {
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,

    pub sui_address: SuiAddress,

    pub gas_price: u64,
    pub commission_rate: u64,

    pub protocol_public_key: Vec<u8>, //AuthorityPublicKeyBytes,
    pub proof_of_possession: Vec<u8>, // AuthoritySignature,

    pub network_public_key: Vec<u8>, // NetworkPublicKey,
    pub worker_public_key: Vec<u8>,  // NetworkPublicKey,

    pub network_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub primary_address: Multiaddr,
    pub worker_address: Multiaddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GenesisChainParameters {
    pub protocol_version: u64,
    pub chain_start_timestamp_ms: u64,
    pub epoch_duration_ms: u64,

    // Stake Subsidy parameters
    pub stake_subsidy_start_epoch: u64,
    pub stake_subsidy_initial_distribution_amount: u64,
    pub stake_subsidy_period_length: u64,
    pub stake_subsidy_decrease_rate: u16,

    // Validator committee parameters
    pub max_validator_count: u64,
    pub min_validator_joining_stake: u64,
    pub validator_low_stake_threshold: u64,
    pub validator_very_low_stake_threshold: u64,
    pub validator_low_stake_grace_period: u64,
}

/// Initial set of parameters for a chain.
#[derive(Serialize, Deserialize)]
pub struct GenesisCeremonyParameters {
    #[serde(default = "GenesisCeremonyParameters::default_timestamp_ms")]
    pub chain_start_timestamp_ms: u64,

    /// protocol version that the chain starts at.
    #[serde(default = "ProtocolVersion::max")]
    pub protocol_version: ProtocolVersion,

    #[serde(default = "GenesisCeremonyParameters::default_allow_insertion_of_extra_objects")]
    pub allow_insertion_of_extra_objects: bool,

    /// The duration of an epoch, in milliseconds.
    #[serde(default = "GenesisCeremonyParameters::default_epoch_duration_ms")]
    pub epoch_duration_ms: u64,

    /// The starting epoch in which stake subsidies start being paid out.
    #[serde(default)]
    pub stake_subsidy_start_epoch: u64,

    /// The amount of stake subsidy to be drawn down per distribution.
    /// This amount decays and decreases over time.
    #[serde(
        default = "GenesisCeremonyParameters::default_initial_stake_subsidy_distribution_amount"
    )]
    pub stake_subsidy_initial_distribution_amount: u64,

    /// Number of distributions to occur before the distribution amount decays.
    #[serde(default = "GenesisCeremonyParameters::default_stake_subsidy_period_length")]
    pub stake_subsidy_period_length: u64,

    /// The rate at which the distribution amount decays at the end of each
    /// period. Expressed in basis points.
    #[serde(default = "GenesisCeremonyParameters::default_stake_subsidy_decrease_rate")]
    pub stake_subsidy_decrease_rate: u16,
    // Most other parameters (e.g. initial gas schedule) should be derived from protocol_version.
}

impl GenesisCeremonyParameters {
    pub fn new() -> Self {
        Self {
            chain_start_timestamp_ms: Self::default_timestamp_ms(),
            protocol_version: ProtocolVersion::MAX,
            allow_insertion_of_extra_objects: true,
            stake_subsidy_start_epoch: 0,
            epoch_duration_ms: Self::default_epoch_duration_ms(),
            stake_subsidy_initial_distribution_amount:
                Self::default_initial_stake_subsidy_distribution_amount(),
            stake_subsidy_period_length: Self::default_stake_subsidy_period_length(),
            stake_subsidy_decrease_rate: Self::default_stake_subsidy_decrease_rate(),
        }
    }

    fn default_timestamp_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn default_allow_insertion_of_extra_objects() -> bool {
        true
    }

    fn default_epoch_duration_ms() -> u64 {
        // 24 hrs
        24 * 60 * 60 * 1000
    }

    fn default_initial_stake_subsidy_distribution_amount() -> u64 {
        // 1M Sui
        1_000_000 * sui_types::gas_coin::MIST_PER_SUI
    }

    fn default_stake_subsidy_period_length() -> u64 {
        // 30 distributions or epochs
        30
    }

    fn default_stake_subsidy_decrease_rate() -> u16 {
        // 10% in basis points
        10000
    }

    fn to_genesis_chain_parameters(&self) -> GenesisChainParameters {
        GenesisChainParameters {
            protocol_version: self.protocol_version.as_u64(),
            stake_subsidy_start_epoch: self.stake_subsidy_start_epoch,
            chain_start_timestamp_ms: self.chain_start_timestamp_ms,
            epoch_duration_ms: self.epoch_duration_ms,
            stake_subsidy_initial_distribution_amount: self
                .stake_subsidy_initial_distribution_amount,
            stake_subsidy_period_length: self.stake_subsidy_period_length,
            stake_subsidy_decrease_rate: self.stake_subsidy_decrease_rate,
            max_validator_count: sui_types::governance::MAX_VALIDATOR_COUNT,
            min_validator_joining_stake: sui_types::governance::MIN_VALIDATOR_JOINING_STAKE_MIST,
            validator_low_stake_threshold:
                sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST,
            validator_very_low_stake_threshold:
                sui_types::governance::VALIDATOR_VERY_LOW_STAKE_THRESHOLD_MIST,
            validator_low_stake_grace_period:
                sui_types::governance::VALIDATOR_LOW_STAKE_GRACE_PERIOD,
        }
    }
}

impl Default for GenesisCeremonyParameters {
    fn default() -> Self {
        Self::new()
    }
}

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
            let signatures = self.signatures.clone().into_values().collect::<Vec<_>>();

            CertifiedCheckpointSummary::new(checkpoint, &signatures, &committee).unwrap()
        };

        let genesis = Genesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            events,
            objects,
        };

        // Verify that all on-chain state was properly created
        self.validate().unwrap();

        genesis
    }

    /// Validates the entire state of the build, no matter what the internal state is (input
    /// collection phase or output phase)
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        self.validate_inputs()?;
        self.validate_output();
        Ok(())
    }

    /// Runs through validation checks on the input values present in the builder
    fn validate_inputs(&self) -> Result<(), anyhow::Error> {
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

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
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

    pub fn save<P: AsRef<Path>>(self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis Builder to {}", path.display());

        fs::create_dir_all(path)?;

        // Write parameters
        let parameters_file = path.join(GENESIS_BUILDER_PARAMETERS_FILE);
        fs::write(parameters_file, serde_yaml::to_vec(&self.parameters)?)?;

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
            let validator_info_bytes = serde_yaml::to_vec(&validator)?;
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

    let protocol_config = ProtocolConfig::get_for_version(parameters.protocol_version);

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

                sui_types::messages::GenesisObject::RawObject {
                    data: object.data,
                    owner: object.owner,
                }
            })
            .collect();

        sui_types::messages::VerifiedTransaction::new_genesis_transaction(genesis_objects)
            .into_inner()
    };

    // execute txn to effects
    let (effects, events, objects) = {
        let mut store = sui_types::in_memory_storage::InMemoryStorage::new(Vec::new());
        let temporary_store = TemporaryStore::new(
            &mut store,
            InputObjects::new(vec![]),
            *genesis_transaction.digest(),
            protocol_config,
        );

        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let enable_move_vm_paranoid_checks = false;
        let move_vm = std::sync::Arc::new(
            adapter::new_move_vm(
                native_functions,
                protocol_config,
                enable_move_vm_paranoid_checks,
            )
            .expect("We defined natives to not fail here"),
        );

        let transaction_data = &genesis_transaction.data().intent_message().value;
        let (kind, signer, gas) = transaction_data.execution_parts();
        let (inner_temp_store, effects, _execution_error) =
            sui_adapter::execution_engine::execute_transaction_to_effects::<
                execution_mode::Normal,
                _,
            >(
                vec![],
                temporary_store,
                kind,
                signer,
                &gas,
                *genesis_transaction.digest(),
                Default::default(),
                &move_vm,
                SuiGasStatus::new_unmetered(protocol_config),
                epoch_data,
                protocol_config,
                metrics,
                false, // enable_expensive_checks
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
    let protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::new(parameters.protocol_version));

    let native_functions = sui_move_natives::all_natives(/* silent */ true);
    // paranoid checks are a last line of defense for malicious code, no need to run them in genesis
    let enable_move_vm_paranoid_checks = false;
    let move_vm = adapter::new_move_vm(
        native_functions.clone(),
        &protocol_config,
        enable_move_vm_paranoid_checks,
    )
    .expect("We defined natives to not fail here");

    for system_package in BuiltInFramework::iter_system_packages() {
        process_package(
            &mut store,
            &move_vm,
            genesis_ctx,
            &system_package.modules(),
            system_package.dependencies().to_vec(),
            &protocol_config,
            metrics.clone(),
        )
        .unwrap();
    }

    for object in input_objects {
        store.insert_object(object.to_owned());
    }

    generate_genesis_system_object(
        &mut store,
        &move_vm,
        validators,
        genesis_ctx,
        parameters,
        token_distribution_schedule,
        metrics,
    )
    .unwrap();

    store.into_inner().into_values().collect()
}

fn process_package(
    store: &mut InMemoryStorage,
    vm: &MoveVM,
    ctx: &mut TxContext,
    modules: &[CompiledModule],
    dependencies: Vec<ObjectID>,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> Result<()> {
    let dependency_objects = store.get_objects(&dependencies);
    // When publishing genesis packages, since the std framework packages all have
    // non-zero addresses, [`Transaction::input_objects_in_compiled_modules`] will consider
    // them as dependencies even though they are not. Hence input_objects contain objects
    // that don't exist on-chain because they are yet to be published.
    #[cfg(debug_assertions)]
    {
        use move_core_types::account_address::AccountAddress;
        use std::collections::HashSet;
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

    let mut temporary_store = TemporaryStore::new(
        &*store,
        InputObjects::new(loaded_dependencies),
        ctx.digest(),
        protocol_config,
    );
    let mut gas_status = SuiGasStatus::new_unmetered(protocol_config);
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
    programmable_transactions::execution::execute::<_, execution_mode::Genesis>(
        protocol_config,
        metrics,
        vm,
        &mut temporary_store,
        ctx,
        &mut gas_status,
        None,
        pt,
    )?;

    let InnerTemporaryStore {
        written, deleted, ..
    } = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

pub fn generate_genesis_system_object(
    store: &mut InMemoryStorage,
    move_vm: &MoveVM,
    genesis_validators: &[GenesisValidatorMetadata],
    genesis_ctx: &mut TxContext,
    genesis_chain_parameters: &GenesisChainParameters,
    token_distribution_schedule: &TokenDistributionSchedule,
    metrics: Arc<LimitsMetrics>,
) -> Result<()> {
    let genesis_digest = genesis_ctx.digest();
    let protocol_config = ProtocolConfig::get_for_version(ProtocolVersion::new(
        genesis_chain_parameters.protocol_version,
    ));
    let mut temporary_store = TemporaryStore::new(
        &*store,
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
        .collect::<Result<_, _>>()?;
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
    programmable_transactions::execution::execute::<_, execution_mode::Genesis>(
        &protocol_config,
        metrics,
        move_vm,
        &mut temporary_store,
        genesis_ctx,
        &mut SuiGasStatus::new_unmetered(&protocol_config),
        None,
        pt,
    )?;

    let InnerTemporaryStore {
        written, deleted, ..
    } = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

const GENESIS_BUILDER_COMMITTEE_DIR: &str = "committee";
const GENESIS_BUILDER_PARAMETERS_FILE: &str = "parameters";
const GENESIS_BUILDER_TOKEN_DISTRIBUTION_SCHEDULE_FILE: &str = "token-distribution-schedule";
const GENESIS_BUILDER_SIGNATURE_DIR: &str = "signatures";
const GENESIS_BUILDER_UNSIGNED_GENESIS_FILE: &str = "unsigned-genesis";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TokenDistributionSchedule {
    pub stake_subsidy_fund_mist: u64,
    pub allocations: Vec<TokenAllocation>,
}

impl TokenDistributionSchedule {
    pub fn validate(&self) {
        let mut total_mist = self.stake_subsidy_fund_mist;

        for allocation in &self.allocations {
            total_mist += allocation.amount_mist;
        }

        if total_mist != TOTAL_SUPPLY_MIST {
            panic!("TokenDistributionSchedule adds up to {total_mist} and not expected {TOTAL_SUPPLY_MIST}");
        }
    }

    fn check_all_stake_operations_are_for_valid_validators<I: IntoIterator<Item = SuiAddress>>(
        &self,
        validators: I,
    ) {
        use std::collections::HashMap;

        let mut validators: HashMap<SuiAddress, u64> =
            validators.into_iter().map(|a| (a, 0)).collect();

        // Check that all allocations are for valid validators, while summing up all allocations
        // for each validator
        for allocation in &self.allocations {
            if let Some(staked_with_validator) = &allocation.staked_with_validator {
                *validators
                    .get_mut(staked_with_validator)
                    .expect("allocation must be staked with valid validator") +=
                    allocation.amount_mist;
            }
        }

        // Check that all validators have sufficient stake allocated to ensure they meet the
        // minimum stake threshold
        let minimum_required_stake = sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST;
        for (validator, stake) in validators {
            if stake < minimum_required_stake {
                panic!("validator {validator} has '{stake}' stake and does not meet the minimum required stake threshold of '{minimum_required_stake}'");
            }
        }
    }

    fn new_for_validators_with_default_allocation<I: IntoIterator<Item = SuiAddress>>(
        validators: I,
    ) -> Self {
        let mut supply = TOTAL_SUPPLY_MIST;
        let default_allocation = sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST;

        let allocations = validators
            .into_iter()
            .map(|a| {
                supply -= default_allocation;
                TokenAllocation {
                    recipient_address: a,
                    amount_mist: default_allocation,
                    staked_with_validator: Some(a),
                }
            })
            .collect();

        let schedule = Self {
            stake_subsidy_fund_mist: supply,
            allocations,
        };

        schedule.validate();
        schedule
    }

    /// Helper to read a TokenDistributionSchedule from a csv file.
    ///
    /// The file is encoded such that the final entry in the CSV file is used to denote the
    /// allocation to the stake subsidy fund. It must be in the following format:
    /// `0x0000000000000000000000000000000000000000000000000000000000000000,<amount to stake subsidy fund>,`
    ///
    /// All entries in a token distribution schedule must add up to 10B Sui.
    pub fn from_csv<R: std::io::Read>(reader: R) -> Result<Self> {
        let mut reader = csv::Reader::from_reader(reader);
        let mut allocations: Vec<TokenAllocation> =
            reader.deserialize().collect::<Result<_, _>>()?;
        assert_eq!(
            TOTAL_SUPPLY_MIST,
            allocations.iter().map(|a| a.amount_mist).sum::<u64>(),
            "Token Distribution Schedule must add up to 10B Sui",
        );
        let stake_subsidy_fund_allocation = allocations.pop().unwrap();
        assert_eq!(
            SuiAddress::default(),
            stake_subsidy_fund_allocation.recipient_address,
            "Final allocation must be for stake subsidy fund",
        );
        assert!(
            stake_subsidy_fund_allocation
                .staked_with_validator
                .is_none(),
            "Can't stake the stake subsidy fund",
        );

        let schedule = Self {
            stake_subsidy_fund_mist: stake_subsidy_fund_allocation.amount_mist,
            allocations,
        };

        schedule.validate();
        Ok(schedule)
    }

    pub fn to_csv<W: std::io::Write>(&self, writer: W) -> Result<()> {
        let mut writer = csv::Writer::from_writer(writer);

        for allocation in &self.allocations {
            writer.serialize(allocation)?;
        }

        writer.serialize(TokenAllocation {
            recipient_address: SuiAddress::default(),
            amount_mist: self.stake_subsidy_fund_mist,
            staked_with_validator: None,
        })?;

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TokenAllocation {
    pub recipient_address: SuiAddress,
    pub amount_mist: u64,

    /// Indicates if this allocation should be staked at genesis and with which validator
    pub staked_with_validator: Option<SuiAddress>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenDistributionScheduleBuilder {
    pool: u64,
    allocations: Vec<TokenAllocation>,
}

impl TokenDistributionScheduleBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            pool: TOTAL_SUPPLY_MIST,
            allocations: vec![],
        }
    }

    pub fn default_allocation_for_validators<I: IntoIterator<Item = SuiAddress>>(
        &mut self,
        validators: I,
    ) {
        let default_allocation = sui_types::governance::VALIDATOR_LOW_STAKE_THRESHOLD_MIST;

        for validator in validators {
            self.add_allocation(TokenAllocation {
                recipient_address: validator,
                amount_mist: default_allocation,
                staked_with_validator: Some(validator),
            });
        }
    }

    pub fn add_allocation(&mut self, allocation: TokenAllocation) {
        self.pool = self.pool.checked_sub(allocation.amount_mist).unwrap();
        self.allocations.push(allocation);
    }

    pub fn build(&self) -> TokenDistributionSchedule {
        let schedule = TokenDistributionSchedule {
            stake_subsidy_fund_mist: self.pool,
            allocations: self.allocations.clone(),
        };

        schedule.validate();
        schedule
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        node::{DEFAULT_COMMISSION_RATE, DEFAULT_VALIDATOR_GAS_PRICE},
        utils, ValidatorInfo,
    };
    use fastcrypto::traits::KeyPair;
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
    fn roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s: Genesis = serde_yaml::from_str(&s).unwrap();
        // cache the digest so that the comparison succeeds.
        from_s.checkpoint_contents.digest();
        assert_eq!(genesis, from_s);
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
            network_address: utils::new_tcp_network_address(),
            p2p_address: utils::new_udp_network_address(),
            narwhal_primary_address: utils::new_udp_network_address(),
            narwhal_worker_address: utils::new_udp_network_address(),
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

    #[test]
    fn genesis_transaction() {
        let builder = crate::builder::ConfigBuilder::new_with_temp_dir();
        let network_config = builder.build();
        let genesis = network_config.genesis;
        let protocol_version = ProtocolVersion::new(genesis.sui_system_object().protocol_version());
        let protocol_config = ProtocolConfig::get_for_version(protocol_version);

        let genesis_transaction = genesis.transaction.clone();

        let mut store = sui_types::in_memory_storage::InMemoryStorage::new(Vec::new());
        let temporary_store = TemporaryStore::new(
            &mut store,
            InputObjects::new(vec![]),
            *genesis_transaction.digest(),
            &protocol_config,
        );

        let enable_move_vm_paranoid_checks = false;
        let native_functions = sui_move_natives::all_natives(/* silent */ true);
        let move_vm = std::sync::Arc::new(
            adapter::new_move_vm(
                native_functions,
                &protocol_config,
                enable_move_vm_paranoid_checks,
            )
            .expect("We defined natives to not fail here"),
        );

        // Use a throwaway metrics registry for genesis transaction execution.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));

        let transaction_data = &genesis_transaction.data().intent_message().value;
        let (kind, signer, gas) = transaction_data.execution_parts();
        let (_inner_temp_store, effects, _execution_error) =
            sui_adapter::execution_engine::execute_transaction_to_effects::<
                execution_mode::Normal,
                _,
            >(
                vec![],
                temporary_store,
                kind,
                signer,
                &gas,
                *genesis_transaction.digest(),
                Default::default(),
                &move_vm,
                SuiGasStatus::new_unmetered(&protocol_config),
                &EpochData::new_test(),
                &protocol_config,
                metrics,
                false, // enable_expensive_checks
            );

        assert_eq!(effects, genesis.effects);
    }
}
