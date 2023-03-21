// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorInfo;
use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use fastcrypto::encoding::{Base64, Encoding, Hex};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::KeyPair;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::{fs, path::Path};
use sui_adapter::adapter::MoveVM;
use sui_adapter::{adapter, execution_mode, programmable_transactions};
use sui_framework::{MoveStdlib, SuiFramework, SystemPackage};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ExecutionDigests, TransactionDigest};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::clock::Clock;
use sui_types::committee::CommitteeWithNetworkMetadata;
use sui_types::crypto::DefaultHash;
use sui_types::crypto::{
    AuthorityKeyPair, AuthorityPublicKeyBytes, AuthoritySignInfo, AuthoritySignature,
    SuiAuthoritySignature, ToFromBytes,
};
use sui_types::epoch_data::EpochData;
use sui_types::gas::SuiGasStatus;
use sui_types::gas_coin::TOTAL_SUPPLY_MIST;
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::message_envelope::Message;
use sui_types::messages::{
    CallArg, Command, InputObjectKind, InputObjects, Transaction, TransactionEffects,
    TransactionEvents,
};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, VerifiedCheckpoint,
};
use sui_types::multiaddr::Multiaddr;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::{
    get_sui_system_state, get_sui_system_state_version, get_sui_system_state_wrapper,
    SuiSystemStateInnerGenesis, SuiSystemStateTrait, SuiSystemStateWrapper, SuiValidatorGenesis,
};
use sui_types::temporary_store::{InnerTemporaryStore, TemporaryStore};
use sui_types::MOVE_STDLIB_ADDRESS;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{
    base_types::TxContext,
    committee::{Committee, EpochId, ProtocolVersion},
    error::SuiResult,
    object::Object,
};
use tracing::trace;

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

    pub fn validator_set(&self) -> Vec<SuiValidatorGenesis> {
        self.sui_system_object().validators.active_validators
    }

    pub fn committee_with_network(&self) -> CommitteeWithNetworkMetadata {
        self.sui_system_object().get_current_epoch_committee()
    }

    // TODO: No need to return SuiResult.
    pub fn committee(&self) -> SuiResult<Committee> {
        Ok(self.committee_with_network().committee)
    }

    pub fn sui_system_wrapper_object(&self) -> SuiSystemStateWrapper {
        get_sui_system_state_wrapper(&self.objects())
            .expect("Sui System State Wrapper object must always exist")
    }

    pub fn sui_system_object(&self) -> SuiSystemStateInnerGenesis {
        get_sui_system_state(&self.objects())
            .expect("Sui System State object must always exist")
            .into_genesis_version()
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

    pub fn validator_set(&self) -> Vec<SuiValidatorGenesis> {
        self.sui_system_object().validators.active_validators
    }

    pub fn sui_system_wrapper_object(&self) -> SuiSystemStateWrapper {
        get_sui_system_state_wrapper(&self.objects())
            .expect("Sui System State Wrapper object must always exist")
    }

    pub fn sui_system_object(&self) -> SuiSystemStateInnerGenesis {
        get_sui_system_state(&self.objects())
            .expect("Sui System State object must always exist")
            .into_genesis_version()
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisValidatorInfo {
    pub info: ValidatorInfo,
    pub proof_of_possession: AuthoritySignature,
}

impl From<GenesisValidatorInfo> for GenesisValidatorMetadata {
    fn from(
        GenesisValidatorInfo {
            info,
            proof_of_possession,
        }: GenesisValidatorInfo,
    ) -> Self {
        let sui_address = info.sui_address();

        Self {
            name: info.name,
            description: info.description,
            image_url: info.image_url,
            project_url: info.project_url,
            sui_address,
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
    pub governance_start_epoch: u64,
    pub chain_start_timestamp_ms: u64,
    pub epoch_duration_ms: u64,
}

/// Initial set of parameters for a chain.
#[derive(Serialize, Deserialize)]
pub struct GenesisCeremonyParameters {
    #[serde(default = "GenesisCeremonyParameters::default_timestamp_ms")]
    pub timestamp_ms: u64,

    /// protocol version that the chain starts at.
    #[serde(default = "ProtocolVersion::max")]
    pub protocol_version: ProtocolVersion,

    #[serde(default = "GenesisCeremonyParameters::default_allow_insertion_of_extra_objects")]
    pub allow_insertion_of_extra_objects: bool,

    /// The starting epoch in which various on-chain governance features take effect. E.g.
    /// - stake subsidies are paid out
    /// - validators with stake less than a 'validator_stake_threshold' are
    ///   kicked from the validator set
    #[serde(default)]
    pub governance_start_epoch: u64,

    /// The duration of an epoch, in milliseconds.
    #[serde(default = "GenesisCeremonyParameters::default_epoch_duration_ms")]
    pub epoch_duration_ms: u64,
    // Most other parameters (e.g. initial gas schedule) should be derived from protocol_version.
}

impl GenesisCeremonyParameters {
    pub fn new() -> Self {
        Self {
            timestamp_ms: Self::default_timestamp_ms(),
            protocol_version: ProtocolVersion::MAX,
            allow_insertion_of_extra_objects: true,
            governance_start_epoch: 0,
            epoch_duration_ms: Self::default_epoch_duration_ms(),
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

    pub fn add_validator_signature(mut self, keypair: &AuthorityKeyPair) -> Self {
        let UnsignedGenesis { checkpoint, .. } = self.build_unsigned_genesis_checkpoint();

        let name = keypair.public().into();
        assert!(
            self.validators.contains_key(&name),
            "provided keypair does not correspond to a validator in the validator set"
        );
        let checkpoint_signature = {
            let intent_msg = IntentMessage::new(
                Intent::default().with_scope(IntentScope::CheckpointSummary),
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
            let signatures = self
                .signatures
                .clone()
                .into_iter()
                .map(|(_, s)| s)
                .collect();

            CertifiedCheckpointSummary::new(checkpoint, signatures, &committee).unwrap()
        };

        let validators = self.validators.into_values().collect::<Vec<_>>();

        // Ensure we have signatures from all validators
        assert_eq!(checkpoint.auth_sig().len(), validators.len() as u64);

        let genesis = Genesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            events,
            objects,
        };

        // Verify that all the validators were properly created onchain
        let system_object = genesis.sui_system_object();
        assert_eq!(system_object.epoch, 0);

        for (validator, onchain_validator) in validators
            .iter()
            .map(|genesis_info| &genesis_info.info)
            .zip(system_object.validators.active_validators.iter())
        {
            let metadata = onchain_validator.verified_metadata();
            assert_eq!(validator.sui_address(), metadata.sui_address);
            assert_eq!(validator.protocol_key(), metadata.sui_pubkey_bytes());
            assert_eq!(validator.name(), &metadata.name);
            assert_eq!(validator.network_address(), &metadata.net_address);
        }

        genesis
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

        // Load Objects
        let mut objects = BTreeMap::new();
        for entry in path.join(GENESIS_BUILDER_OBJECT_DIR).read_dir_utf8()? {
            let entry = entry?;
            if entry.file_name().starts_with('.') {
                continue;
            }

            let path = entry.path();
            let object_bytes = fs::read(path)?;
            let object: Object = serde_yaml::from_slice(&object_bytes)?;
            objects.insert(object.id(), object);
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
            let sigs: AuthoritySignInfo = bcs::from_bytes(&signature_bytes)?;
            signatures.insert(sigs.authority, sigs);
        }

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
                serde_yaml::from_slice(&validator_info_bytes)?;
            committee.insert(validator_info.info.protocol_key(), validator_info);
        }

        let mut builder = Self {
            parameters,
            token_distribution_schedule,
            objects,
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

        // Write Objects
        let object_dir = path.join(GENESIS_BUILDER_OBJECT_DIR);
        fs::create_dir_all(&object_dir)?;

        for (_id, object) in self.objects {
            let object_bytes = serde_yaml::to_vec(&object)?;
            let hex_digest = Hex::encode(object.id());
            fs::write(object_dir.join(hex_digest), object_bytes)?;
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

fn get_genesis_context(epoch_data: &EpochData) -> TxContext {
    TxContext::new(
        &SuiAddress::default(),
        &TransactionDigest::genesis(),
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

    let protocol_config = ProtocolConfig::get_for_version(parameters.protocol_version);
    let epoch_data = EpochData::new_genesis(parameters.timestamp_ms);

    let mut genesis_ctx = get_genesis_context(&epoch_data);

    // Get Move and Sui Framework
    let modules = [
        (
            MoveStdlib::as_modules(),
            MoveStdlib::transitive_dependencies(),
        ),
        (
            SuiFramework::as_modules(),
            SuiFramework::transitive_dependencies(),
        ),
    ];

    let objects = create_genesis_objects(
        &mut genesis_ctx,
        &modules,
        objects,
        validators,
        parameters,
        token_distribution_schedule,
    );

    let (genesis_transaction, genesis_effects, genesis_events, objects) =
        create_genesis_transaction(objects, &protocol_config, &epoch_data);
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
        timestamp_ms: parameters.timestamp_ms,
        version_specific_data: Vec::new(),
        checkpoint_commitments: Default::default(),
    };

    (checkpoint, contents)
}

fn create_genesis_transaction(
    objects: Vec<Object>,
    protocol_config: &ProtocolConfig,
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

        let native_functions = sui_framework::natives::all_natives(
            sui_types::MOVE_STDLIB_ADDRESS,
            sui_types::SUI_FRAMEWORK_ADDRESS,
        );
        let move_vm = std::sync::Arc::new(
            adapter::new_move_vm(native_functions, protocol_config)
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
                SuiGasStatus::new_unmetered(),
                epoch_data,
                protocol_config,
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
    modules: &[(Vec<CompiledModule>, Vec<ObjectID>)],
    input_objects: &[Object],
    validators: &[GenesisValidatorInfo],
    parameters: &GenesisCeremonyParameters,
    token_distribution_schedule: &TokenDistributionSchedule,
) -> Vec<Object> {
    let mut store = InMemoryStorage::new(Vec::new());
    let protocol_config = ProtocolConfig::get_for_version(parameters.protocol_version);

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let move_vm = adapter::new_move_vm(native_functions.clone(), &protocol_config)
        .expect("We defined natives to not fail here");

    for (modules, dependencies) in modules {
        process_package(
            &mut store,
            &move_vm,
            genesis_ctx,
            modules.to_owned(),
            dependencies.to_owned(),
            &protocol_config,
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
    )
    .unwrap();

    store.into_inner().into_values().collect()
}

fn process_package(
    store: &mut InMemoryStorage,
    vm: &MoveVM,
    ctx: &mut TxContext,
    modules: Vec<CompiledModule>,
    dependencies: Vec<ObjectID>,
    protocol_config: &ProtocolConfig,
) -> Result<()> {
    let dependency_objects = store.get_objects(&dependencies);
    // When publishing genesis packages, since the std framework packages all have
    // non-zero addresses, [`Transaction::input_objects_in_compiled_modules`] will consider
    // them as dependencies even though they are not. Hence input_objects contain objects
    // that don't exist on-chain because they are yet to be published.
    #[cfg(debug_assertions)]
    {
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
                    || to_be_published_addresses.contains(dependency))
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

    debug_assert!(ctx.digest() == TransactionDigest::genesis());
    let mut temporary_store = TemporaryStore::new(
        &*store,
        InputObjects::new(loaded_dependencies),
        ctx.digest(),
        protocol_config,
    );
    let mut gas_status = SuiGasStatus::new_unmetered();
    let module_bytes = modules
        .into_iter()
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
    programmable_transactions::execution::execute::<_, _, execution_mode::Genesis>(
        protocol_config,
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
    committee: &[GenesisValidatorInfo],
    genesis_ctx: &mut TxContext,
    parameters: &GenesisCeremonyParameters,
    token_distribution_schedule: &TokenDistributionSchedule,
) -> Result<()> {
    let genesis_digest = genesis_ctx.digest();
    let protocol_config = ProtocolConfig::get_for_version(parameters.protocol_version);
    let system_state_version = get_sui_system_state_version(parameters.protocol_version);
    let mut temporary_store = TemporaryStore::new(
        &*store,
        InputObjects::new(vec![]),
        genesis_digest,
        &protocol_config,
    );

    let genesis_chain_parameters = GenesisChainParameters {
        governance_start_epoch: parameters.governance_start_epoch,
        chain_start_timestamp_ms: parameters.timestamp_ms,
        epoch_duration_ms: parameters.epoch_duration_ms,
    };

    let genesis_validators = committee
        .iter()
        .cloned()
        .map(GenesisValidatorMetadata::from)
        .collect::<Vec<_>>();

    token_distribution_schedule.validate();
    token_distribution_schedule.check_all_stake_operations_are_for_valid_validators(
        genesis_validators.iter().map(|v| v.sui_address),
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
            CallArg::Pure(bcs::to_bytes(&parameters.protocol_version.as_u64()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&system_state_version).unwrap()),
        ]
        .into_iter()
        .map(|a| builder.input(a))
        .collect::<Result<_, _>>()?;
        arguments.append(&mut call_arg_arguments);
        builder.programmable_move_call(
            SUI_FRAMEWORK_ADDRESS.into(),
            ident_str!("genesis").to_owned(),
            ident_str!("create").to_owned(),
            vec![],
            arguments,
        );
        builder.finish()
    };
    programmable_transactions::execution::execute::<_, _, execution_mode::Genesis>(
        &protocol_config,
        move_vm,
        &mut temporary_store,
        genesis_ctx,
        &mut SuiGasStatus::new_unmetered(),
        None,
        pt,
    )?;

    let InnerTemporaryStore {
        written, deleted, ..
    } = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

const GENESIS_BUILDER_OBJECT_DIR: &str = "objects";
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
        let minimum_required_stake =
            sui_types::governance::MINIMUM_VALIDATOR_STAKE_SUI * sui_types::gas_coin::MIST_PER_SUI;
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
        let default_allocation =
            sui_types::governance::MINIMUM_VALIDATOR_STAKE_SUI * sui_types::gas_coin::MIST_PER_SUI;

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
    /// '0x0000000000000000000000000000000000000000000000000000000000000000,<amount to stake subsidy fund>,'
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{genesis_config::GenesisConfig, utils, ValidatorInfo};
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

        let genesis_config = GenesisConfig::for_local_testing();
        let (_account_keys, objects) = genesis_config.generate_accounts(rand::rngs::OsRng).unwrap();

        let key: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let worker_key: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let account_key: AccountKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let network_key: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let validator = ValidatorInfo {
            name: "0".into(),
            protocol_key: key.public().into(),
            worker_key: worker_key.public().clone(),
            account_key: account_key.public().clone().into(),
            network_key: network_key.public().clone(),
            gas_price: 1,
            commission_rate: 0,
            network_address: utils::new_tcp_network_address(),
            p2p_address: utils::new_udp_network_address(),
            narwhal_primary_address: utils::new_udp_network_address(),
            narwhal_worker_address: utils::new_udp_network_address(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
        };
        let pop = generate_proof_of_possession(&key, account_key.public().into());
        let builder = Builder::new()
            .add_objects(objects)
            .add_validator(validator, pop);
        builder.save(dir.path()).unwrap();
        Builder::load(dir.path()).unwrap();
    }

    #[test]
    fn genesis_transaction() {
        let dir = tempfile::TempDir::new().unwrap();
        let builder = crate::builder::ConfigBuilder::new(&dir);
        let protocol_version = builder.protocol_version;
        let protocol_config = ProtocolConfig::get_for_version(protocol_version);
        let network_config = builder.build();
        let genesis = network_config.genesis;

        let genesis_transaction = genesis.transaction.clone();

        let mut store = sui_types::in_memory_storage::InMemoryStorage::new(Vec::new());
        let temporary_store = TemporaryStore::new(
            &mut store,
            InputObjects::new(vec![]),
            *genesis_transaction.digest(),
            &protocol_config,
        );

        let native_functions = sui_framework::natives::all_natives(
            sui_types::MOVE_STDLIB_ADDRESS,
            sui_types::SUI_FRAMEWORK_ADDRESS,
        );
        let move_vm = std::sync::Arc::new(
            adapter::new_move_vm(native_functions, &protocol_config)
                .expect("We defined natives to not fail here"),
        );

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
                SuiGasStatus::new_unmetered(),
                &EpochData::new_test(),
                &protocol_config,
            );

        assert_eq!(effects, genesis.effects);
    }
}
