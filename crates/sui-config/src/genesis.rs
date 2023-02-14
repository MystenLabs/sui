// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorInfo;
use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use fastcrypto::encoding::{Base64, Encoding, Hex};
use fastcrypto::hash::{HashFunction, Sha3_256};
use fastcrypto::traits::KeyPair;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::native_functions::NativeFunctionTable;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::{fs, path::Path};
use sui_adapter::adapter::MoveVM;
use sui_adapter::{adapter, execution_mode};
use sui_types::base_types::{ExecutionDigests, TransactionDigest};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::clock::Clock;
use sui_types::crypto::{
    AuthorityKeyPair, AuthorityPublicKeyBytes, AuthoritySignInfo, AuthoritySignature,
    AuthorityStrongQuorumSignInfo, SuiAuthoritySignature, ToFromBytes,
};
use sui_types::gas::SuiGasStatus;
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::message_envelope::Message;
use sui_types::messages::{CallArg, TransactionEffects};
use sui_types::messages::{CertifiedTransaction, Transaction};
use sui_types::messages::{InputObjects, SignedTransaction};
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, VerifiedCheckpoint,
};
use sui_types::object::Owner;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::temporary_store::{InnerTemporaryStore, TemporaryStore};
use sui_types::MOVE_STDLIB_ADDRESS;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{
    base_types::TxContext,
    committee::{Committee, EpochId, ProtocolVersion},
    error::SuiResult,
    object::Object,
    sui_serde::AuthSignature,
};
use tracing::trace;

#[derive(Clone, Debug)]
pub struct Genesis {
    checkpoint: CertifiedCheckpointSummary,
    checkpoint_contents: CheckpointContents,
    transaction: CertifiedTransaction,
    effects: TransactionEffects,
    objects: Vec<Object>,
    validator_set: Vec<ValidatorInfo>,
}

// Hand implement PartialEq in order to get around the fact that AuthSigs don't impl Eq
impl PartialEq for Genesis {
    fn eq(&self, other: &Self) -> bool {
        self.transaction.data() == other.transaction.data()
            && {
                let this = self.transaction.auth_sig();
                let other = other.transaction.auth_sig();

                this.epoch == other.epoch
                    && this.signature.as_ref() == other.signature.as_ref()
                    && this.signers_map == other.signers_map
            }
            && self.effects == other.effects
            && self.objects == other.objects
            && self.validator_set == other.validator_set
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

    pub fn transaction(&self) -> &CertifiedTransaction {
        &self.transaction
    }

    pub fn effects(&self) -> &TransactionEffects {
        &self.effects
    }

    pub fn checkpoint(&self) -> VerifiedCheckpoint {
        VerifiedCheckpoint::new(self.checkpoint.clone(), &self.committee().unwrap()).unwrap()
    }

    pub fn checkpoint_contents(&self) -> &CheckpointContents {
        &self.checkpoint_contents
    }

    pub fn epoch(&self) -> EpochId {
        0
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        &self.validator_set
    }

    pub fn into_validator_set(self) -> Vec<ValidatorInfo> {
        self.validator_set
    }

    pub fn committee(&self) -> SuiResult<Committee> {
        Ok(self
            .sui_system_object()
            .get_current_epoch_committee()
            .committee)
    }

    pub fn sui_system_object(&self) -> SuiSystemState {
        let sui_system_object = self
            .objects()
            .iter()
            .find(|o| o.id() == sui_types::SUI_SYSTEM_STATE_OBJECT_ID)
            .expect("Sui System State object must always exist");
        let move_object = sui_system_object
            .data
            .try_as_move()
            .expect("Sui System State object must be a Move object");
        let result = bcs::from_bytes::<SuiSystemState>(move_object.contents())
            .expect("Sui System State object deserialization cannot fail");
        result
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

    pub fn sha3(&self) -> [u8; 32] {
        use std::io::Write;

        let mut digest = Sha3_256::default();
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
        struct RawGeneis<'a> {
            checkpoint: &'a CertifiedCheckpointSummary,
            checkpoint_contents: &'a CheckpointContents,
            transaction: &'a CertifiedTransaction,
            effects: &'a TransactionEffects,
            objects: &'a [Object],
            validator_set: &'a [ValidatorInfo],
        }

        let raw_genesis = RawGeneis {
            checkpoint: &self.checkpoint,
            checkpoint_contents: &self.checkpoint_contents,
            transaction: &self.transaction,
            effects: &self.effects,
            objects: &self.objects,
            validator_set: &self.validator_set,
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
        struct RawGeneis {
            checkpoint: CertifiedCheckpointSummary,
            checkpoint_contents: CheckpointContents,
            transaction: CertifiedTransaction,
            effects: TransactionEffects,
            objects: Vec<Object>,
            validator_set: Vec<ValidatorInfo>,
        }

        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Base64::decode(&s).map_err(|e| Error::custom(e.to_string()))?
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            data
        };

        let RawGeneis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            objects,
            validator_set,
        } = bcs::from_bytes(&bytes).map_err(|e| Error::custom(e.to_string()))?;

        Ok(Genesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            objects,
            validator_set,
        })
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisValidatorInfo {
    pub info: ValidatorInfo,
    #[serde_as(as = "AuthSignature")]
    pub proof_of_possession: AuthoritySignature,
}

/// Initial set of parameters for a chain.
#[derive(Serialize, Deserialize)]
pub struct GenesisChainParameters {
    pub timestamp_ms: u64,
    // In the future we can add the initial gas schedule or other parameters here
}

impl GenesisChainParameters {
    pub fn new() -> Self {
        Self {
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

impl Default for GenesisChainParameters {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Builder {
    parameters: GenesisChainParameters,
    objects: BTreeMap<ObjectID, Object>,
    validators: BTreeMap<AuthorityPublicKeyBytes, GenesisValidatorInfo>,
    // Validator signatures over 0: checkpoint 1: genesis transaction
    // TODO remove the need to have a sig on the transaction
    signatures: BTreeMap<AuthorityPublicKeyBytes, (AuthoritySignInfo, AuthoritySignInfo)>,
    built_genesis: Option<(
        CheckpointSummary,
        CheckpointContents,
        Transaction,
        TransactionEffects,
        Vec<Object>,
    )>,
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
            objects: Default::default(),
            validators: Default::default(),
            signatures: Default::default(),
            built_genesis: None,
        }
    }

    pub fn with_parameters(mut self, parameters: GenesisChainParameters) -> Self {
        self.parameters = parameters;
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
        let (checkpoint, _checkpoint_contents, transaction, _effects, _objects) =
            self.build_unsigned_genesis_checkpoint();

        let name = keypair.public().into();
        assert!(
            self.validators.contains_key(&name),
            "provided keypair does not correspond to a validator in the validator set"
        );
        let checkpoint_signature = {
            let signature = AuthoritySignature::new(&checkpoint, checkpoint.epoch, keypair);
            AuthoritySignInfo {
                epoch: checkpoint.epoch,
                authority: name,
                signature,
            }
        };

        let transaction_signature =
            SignedTransaction::new(checkpoint.epoch, transaction.into_data(), keypair, name)
                .auth_sig()
                .clone();

        self.signatures
            .insert(name, (checkpoint_signature, transaction_signature));

        self
    }

    pub fn unsigned_genesis_checkpoint(
        &self,
    ) -> Option<(
        CheckpointSummary,
        CheckpointContents,
        Transaction,
        TransactionEffects,
        Vec<Object>,
    )> {
        self.built_genesis.clone()
    }

    pub fn build_unsigned_genesis_checkpoint(
        &mut self,
    ) -> (
        CheckpointSummary,
        CheckpointContents,
        Transaction,
        TransactionEffects,
        Vec<Object>,
    ) {
        if let Some(built_genesis) = &self.built_genesis {
            return built_genesis.clone();
        }

        let objects = self
            .objects
            .clone()
            .into_iter()
            .map(|(_, o)| o)
            .collect::<Vec<_>>();
        let validators = self
            .validators
            .clone()
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();

        self.built_genesis = Some(build_unsigned_genesis_data(
            &self.parameters,
            &validators,
            &objects,
        ));

        self.built_genesis.clone().unwrap()
    }

    fn committee(objects: &[Object]) -> Committee {
        let sui_system_object = objects
            .iter()
            .find(|o| o.id() == sui_types::SUI_SYSTEM_STATE_OBJECT_ID)
            .expect("Sui System State object must always exist");
        let move_object = sui_system_object
            .data
            .try_as_move()
            .expect("Sui System State object must be a Move object");
        let result = bcs::from_bytes::<SuiSystemState>(move_object.contents())
            .expect("Sui System State object deserialization cannot fail");
        result.get_current_epoch_committee().committee
    }

    pub fn build(mut self) -> Genesis {
        let (checkpoint, checkpoint_contents, transaction, effects, objects) =
            self.build_unsigned_genesis_checkpoint();

        let committee = Self::committee(&objects);

        let transaction = {
            let signatures = self
                .signatures
                .clone()
                .into_iter()
                .map(|(_, (_, s))| s)
                .collect();

            CertifiedTransaction::new(transaction.into_data(), signatures, &committee).unwrap()
        };

        let checkpoint = {
            let signatures = self
                .signatures
                .clone()
                .into_iter()
                .map(|(_, (s, _))| s)
                .collect();

            CertifiedCheckpointSummary {
                summary: checkpoint,
                auth_signature: AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(
                    signatures, &committee,
                )
                .unwrap(),
            }
        };

        let validators = self
            .validators
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();

        // Ensure we have signatures from all validators
        assert_eq!(checkpoint.auth_signature.len(), validators.len() as u64);

        let genesis = Genesis {
            checkpoint,
            checkpoint_contents,
            transaction,
            effects,
            objects,
            validator_set: validators
                .into_iter()
                .map(|genesis_info| genesis_info.info)
                .collect::<Vec<_>>(),
        };

        // Verify that all the validators were properly created onchain
        let system_object = genesis.sui_system_object();
        assert_eq!(system_object.epoch, 0);

        for (validator, onchain_validator) in genesis
            .validator_set()
            .iter()
            .zip(system_object.validators.active_validators.iter())
        {
            assert_eq!(validator.stake(), onchain_validator.stake_amount);
            assert_eq!(
                validator.sui_address().to_vec(),
                onchain_validator.metadata.sui_address.to_vec(),
            );
            assert_eq!(
                validator.protocol_key().as_ref().to_vec(),
                onchain_validator.metadata.pubkey_bytes,
            );
            assert_eq!(validator.name().as_bytes(), onchain_validator.metadata.name);
            assert_eq!(
                validator.network_address().to_vec(),
                onchain_validator.metadata.net_address
            );
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
            let sigs: (AuthoritySignInfo, AuthoritySignInfo) = bcs::from_bytes(&signature_bytes)?;
            signatures.insert(sigs.0.authority, sigs);
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

        let unsigned_genesis_file = path.join(GENESIS_BUILDER_UNSIGNED_GENESIS_FILE);
        let loaded_genesis = if unsigned_genesis_file.exists() {
            let unsinged_genesis_bytes = fs::read(unsigned_genesis_file)?;
            let loaded_genesis: (
                CheckpointSummary,
                CheckpointContents,
                Transaction,
                TransactionEffects,
                Vec<Object>,
            ) = bcs::from_bytes(&unsinged_genesis_bytes)?;
            Some(loaded_genesis)
        } else {
            None
        };

        // Verify it matches
        if let Some(loaded_genesis) = &loaded_genesis {
            let objects = objects
                .clone()
                .into_iter()
                .map(|(_, o)| o)
                .collect::<Vec<_>>();
            let validators = committee
                .clone()
                .into_iter()
                .map(|(_, v)| v)
                .collect::<Vec<_>>();

            let built = build_unsigned_genesis_data(&parameters, &validators, &objects);
            assert_eq!(
                &built, loaded_genesis,
                "loaded genesis does not match built genesis"
            );
        }

        Ok(Self {
            parameters,
            objects,
            validators: committee,
            signatures,
            built_genesis: loaded_genesis,
        })
    }

    pub fn save<P: AsRef<Path>>(self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis Builder to {}", path.display());

        fs::create_dir_all(path)?;

        // Write parameters
        let parameters_file = path.join(GENESIS_BUILDER_PARAMETERS_FILE);
        fs::write(parameters_file, serde_yaml::to_vec(&self.parameters)?)?;

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

fn build_unsigned_genesis_data(
    parameters: &GenesisChainParameters,
    validators: &[GenesisValidatorInfo],
    objects: &[Object],
) -> (
    CheckpointSummary,
    CheckpointContents,
    Transaction,
    TransactionEffects,
    Vec<Object>,
) {
    let mut genesis_ctx = sui_adapter::genesis::get_genesis_context();

    // Get Move and Sui Framework
    let modules = [
        sui_framework::get_move_stdlib(),
        sui_framework::get_sui_framework(),
    ];

    let objects = create_genesis_objects(
        &mut genesis_ctx,
        &modules,
        objects,
        validators,
        parameters.timestamp_ms,
    );

    let (genesis_transaction, genesis_effects, objects) = create_genesis_transaction(objects);
    let (checkpoint, checkpoint_contents) =
        create_genesis_checkpoint(parameters, &genesis_transaction, &genesis_effects);

    (
        checkpoint,
        checkpoint_contents,
        genesis_transaction,
        genesis_effects,
        objects,
    )
}

fn create_genesis_checkpoint(
    parameters: &GenesisChainParameters,
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
        content_digest: contents.digest(),
        previous_digest: None,
        epoch_rolling_gas_cost_summary: Default::default(),
        end_of_epoch_data: None,
        timestamp_ms: parameters.timestamp_ms,
        version_specific_data: Vec::new(),
    };

    (checkpoint, contents)
}

fn create_genesis_transaction(
    objects: Vec<Object>,
) -> (Transaction, TransactionEffects, Vec<Object>) {
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
    let (effects, objects) = {
        let mut store = sui_types::in_memory_storage::InMemoryStorage::new(Vec::new());
        let temporary_store = TemporaryStore::new(
            &mut store,
            InputObjects::new(vec![]),
            *genesis_transaction.digest(),
        );

        let native_functions = sui_framework::natives::all_natives(
            sui_types::MOVE_STDLIB_ADDRESS,
            sui_types::SUI_FRAMEWORK_ADDRESS,
        );
        let move_vm = std::sync::Arc::new(
            adapter::new_move_vm(native_functions.clone())
                .expect("We defined natives to not fail here"),
        );

        let transaction_data = genesis_transaction.data().intent_message.value.clone();
        let signer = transaction_data.sender();
        let gas = transaction_data.gas();
        let (inner_temp_store, effects, _execution_error) =
            sui_adapter::execution_engine::execute_transaction_to_effects::<
                execution_mode::Normal,
                _,
            >(
                vec![],
                temporary_store,
                transaction_data.kind,
                signer,
                gas,
                *genesis_transaction.digest(),
                Default::default(),
                &move_vm,
                &native_functions,
                SuiGasStatus::new_unmetered(),
                0,
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
        (effects, objects)
    };

    (genesis_transaction, effects, objects)
}

fn create_genesis_objects(
    genesis_ctx: &mut TxContext,
    modules: &[Vec<CompiledModule>],
    input_objects: &[Object],
    validators: &[GenesisValidatorInfo],
    epoch_start_timestamp_ms: u64,
) -> Vec<Object> {
    let mut store = InMemoryStorage::new(Vec::new());

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let move_vm = adapter::new_move_vm(native_functions.clone())
        .expect("We defined natives to not fail here");

    for modules in modules {
        process_package(
            &mut store,
            &native_functions,
            genesis_ctx,
            modules.to_owned(),
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
        epoch_start_timestamp_ms,
    )
    .unwrap();

    store
        .into_inner()
        .into_iter()
        .map(|(_id, object)| object)
        .collect()
}

fn process_package(
    store: &mut InMemoryStorage,
    // mv: &MoveVM,
    native_functions: &NativeFunctionTable,
    ctx: &mut TxContext,
    modules: Vec<CompiledModule>,
) -> Result<()> {
    let inputs = Transaction::input_objects_in_compiled_modules(&modules);
    let ids: Vec<_> = inputs.iter().map(|kind| kind.object_id()).collect();
    let input_objects = store.get_objects(&ids[..]);
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
            inputs
                .iter()
                .zip(input_objects.iter())
                .all(|(kind, obj_opt)| obj_opt.is_some()
                    || to_be_published_addresses.contains(&kind.object_id()))
        );
    }
    let filtered = inputs
        .into_iter()
        .zip(input_objects.into_iter())
        .filter_map(|(input, object_opt)| object_opt.map(|object| (input, object.to_owned())))
        .collect::<Vec<_>>();

    debug_assert!(ctx.digest() == TransactionDigest::genesis());
    let mut temporary_store =
        TemporaryStore::new(&*store, InputObjects::new(filtered), ctx.digest());
    let package_id = ObjectID::from(*modules[0].self_id().address());
    let natives = native_functions.clone();
    let mut gas_status = SuiGasStatus::new_unmetered();
    let vm = adapter::verify_and_link(
        &temporary_store,
        &modules,
        package_id,
        natives,
        gas_status.create_move_gas_status(),
    )?;
    adapter::store_package_and_init_modules(
        &mut temporary_store,
        &vm,
        modules,
        ctx,
        gas_status.create_move_gas_status(),
    )?;

    let (
        InnerTemporaryStore {
            written, deleted, ..
        },
        _events,
    ) = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

pub fn generate_genesis_system_object(
    store: &mut InMemoryStorage,
    move_vm: &MoveVM,
    committee: &[GenesisValidatorInfo],
    genesis_ctx: &mut TxContext,
    epoch_start_timestamp_ms: u64,
) -> Result<()> {
    let genesis_digest = genesis_ctx.digest();
    let mut temporary_store =
        TemporaryStore::new(&*store, InputObjects::new(vec![]), genesis_digest);

    let mut pubkeys = Vec::new();
    let mut network_pubkeys = Vec::new();
    let mut worker_pubkeys = Vec::new();
    let mut proof_of_possessions = Vec::new();
    let mut sui_addresses = Vec::new();
    let mut network_addresses = Vec::new();
    let mut consensus_addresses = Vec::new();
    let mut worker_addresses = Vec::new();
    let mut names = Vec::new();
    let mut descriptions = Vec::new();
    let mut image_url = Vec::new();
    let mut project_url = Vec::new();
    let mut stakes = Vec::new();
    let mut gas_prices = Vec::new();
    let mut commission_rates = Vec::new();

    for GenesisValidatorInfo {
        info: validator,
        proof_of_possession,
    } in committee
    {
        pubkeys.push(validator.protocol_key());
        network_pubkeys.push(validator.network_key().as_bytes().to_vec());
        worker_pubkeys.push(validator.worker_key().as_bytes().to_vec());
        proof_of_possessions.push(proof_of_possession.as_ref().to_vec());
        sui_addresses.push(validator.sui_address());
        network_addresses.push(validator.network_address());
        consensus_addresses.push(validator.narwhal_primary_address());
        worker_addresses.push(validator.narwhal_worker_address());
        names.push(validator.name().to_owned().into_bytes());
        descriptions.push(validator.description.clone().into_bytes());
        image_url.push(validator.image_url.clone().into_bytes());
        project_url.push(validator.project_url.clone().into_bytes());
        stakes.push(validator.stake());
        gas_prices.push(validator.gas_price());
        commission_rates.push(validator.commission_rate());
    }

    adapter::execute::<execution_mode::Normal, _, _>(
        move_vm,
        &mut temporary_store,
        ModuleId::new(SUI_FRAMEWORK_ADDRESS, ident_str!("genesis").to_owned()),
        &ident_str!("create").to_owned(),
        vec![],
        vec![
            CallArg::Pure(bcs::to_bytes(&pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&network_pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&worker_pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&proof_of_possessions).unwrap()),
            CallArg::Pure(bcs::to_bytes(&sui_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&names).unwrap()),
            CallArg::Pure(bcs::to_bytes(&descriptions).unwrap()),
            CallArg::Pure(bcs::to_bytes(&image_url).unwrap()),
            CallArg::Pure(bcs::to_bytes(&project_url).unwrap()),
            CallArg::Pure(bcs::to_bytes(&network_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&consensus_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&worker_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&stakes).unwrap()),
            CallArg::Pure(bcs::to_bytes(&gas_prices).unwrap()),
            CallArg::Pure(bcs::to_bytes(&commission_rates).unwrap()),
            CallArg::Pure(bcs::to_bytes(&ProtocolVersion::MIN.0).unwrap()),
            CallArg::Pure(bcs::to_bytes(&epoch_start_timestamp_ms).unwrap()),
        ],
        SuiGasStatus::new_unmetered().create_move_gas_status(),
        genesis_ctx,
    )?;

    let (
        InnerTemporaryStore {
            written, deleted, ..
        },
        _events,
    ) = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

const GENESIS_BUILDER_OBJECT_DIR: &str = "objects";
const GENESIS_BUILDER_COMMITTEE_DIR: &str = "committee";
const GENESIS_BUILDER_PARAMETERS_FILE: &str = "parameters";
const GENESIS_BUILDER_SIGNATURE_DIR: &str = "signatures";
const GENESIS_BUILDER_UNSIGNED_GENESIS_FILE: &str = "unsigned-genesis";

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
    fn roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s = serde_yaml::from_str(&s).unwrap();
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
            stake: 1,
            delegation: 0,
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
        let network_config = crate::builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let genesis_transaction = genesis.transaction.clone();

        let mut store = sui_types::in_memory_storage::InMemoryStorage::new(Vec::new());
        let temporary_store = TemporaryStore::new(
            &mut store,
            InputObjects::new(vec![]),
            *genesis_transaction.digest(),
        );

        let native_functions = sui_framework::natives::all_natives(
            sui_types::MOVE_STDLIB_ADDRESS,
            sui_types::SUI_FRAMEWORK_ADDRESS,
        );
        let move_vm = std::sync::Arc::new(
            adapter::new_move_vm(native_functions.clone())
                .expect("We defined natives to not fail here"),
        );

        let transaction_data = genesis_transaction.data().intent_message.value.clone();
        let signer = transaction_data.sender();
        let gas = transaction_data.gas();
        let (_inner_temp_store, effects, _execution_error) =
            sui_adapter::execution_engine::execute_transaction_to_effects::<
                execution_mode::Normal,
                _,
            >(
                vec![],
                temporary_store,
                transaction_data.kind,
                signer,
                gas,
                *genesis_transaction.digest(),
                Default::default(),
                &move_vm,
                &native_functions,
                SuiGasStatus::new_unmetered(),
                0,
            );

        assert_eq!(effects, genesis.effects);
    }
}
