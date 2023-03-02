// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, ObjectID, SuiAddress};
use crate::collection_types::{VecMap, VecSet};
use crate::committee::{Committee, CommitteeWithNetAddresses, ProtocolVersion, StakeUnit};
use crate::crypto::AuthorityPublicKeyBytes;
use crate::dynamic_field::{derive_dynamic_field_id, Field};
use crate::error::SuiError;
use crate::storage::ObjectStore;
use crate::{balance::Balance, id::UID, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID};
use anemo::PeerId;
use anyhow::Result;
use fastcrypto::traits::ToFromBytes;
use move_core_types::language_storage::TypeTag;
use move_core_types::value::MoveTypeLayout;
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use move_vm_types::values::Value;
use multiaddr::Multiaddr;
use narwhal_config::{Committee as NarwhalCommittee, WorkerCache, WorkerIndex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

const SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME: &IdentStr = ident_str!("SuiSystemState");
pub const SUI_SYSTEM_MODULE_NAME: &IdentStr = ident_str!("sui_system");
pub const ADVANCE_EPOCH_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch");
pub const ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME: &IdentStr = ident_str!("advance_epoch_safe_mode");
pub const CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME: &IdentStr =
    ident_str!("consensus_commit_prologue");

const E_METADATA_INVALID_PUBKEY: u64 = 1;
const E_METADATA_INVALID_NET_PUBKEY: u64 = 2;
const E_METADATA_INVALID_WORKER_PUBKEY: u64 = 3;
const E_METADATA_INVALID_NET_ADDR: u64 = 4;
const E_METADATA_INVALID_P2P_ADDR: u64 = 5;
const E_METADATA_INVALID_CONSENSUS_ADDR: u64 = 6;
const E_METADATA_INVALID_WORKER_ADDR: u64 = 7;

/// Rust version of the Move sui::sui_system::SystemParameters type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SystemParameters {
    pub min_validator_stake: u64,
    pub max_validator_candidate_count: u64,
}

/// Rust version of the Move std::option::Option type.
/// Putting it in this file because it's only used here.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct MoveOption<T> {
    pub vec: Vec<T>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct ValidatorMetadata {
    pub sui_address: SuiAddress,
    pub protocol_pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub worker_pubkey_bytes: Vec<u8>,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: Vec<u8>,
    pub p2p_address: Vec<u8>,
    pub consensus_address: Vec<u8>,
    pub worker_address: Vec<u8>,
    pub next_epoch_protocol_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    pub next_epoch_network_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_worker_pubkey_bytes: Option<Vec<u8>>,
    pub next_epoch_net_address: Option<Vec<u8>>,
    pub next_epoch_p2p_address: Option<Vec<u8>>,
    pub next_epoch_consensus_address: Option<Vec<u8>>,
    pub next_epoch_worker_address: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct VerifiedValidatorMetadata {
    pub sui_address: SuiAddress,
    pub protocol_pubkey: narwhal_crypto::PublicKey,
    pub network_pubkey: narwhal_crypto::NetworkPublicKey,
    pub worker_pubkey: narwhal_crypto::NetworkPublicKey,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub consensus_address: Multiaddr,
    pub worker_address: Multiaddr,
    pub next_epoch_protocol_pubkey: Option<narwhal_crypto::PublicKey>,
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    pub next_epoch_network_pubkey: Option<narwhal_crypto::NetworkPublicKey>,
    pub next_epoch_worker_pubkey: Option<narwhal_crypto::NetworkPublicKey>,
    pub next_epoch_net_address: Option<Multiaddr>,
    pub next_epoch_p2p_address: Option<Multiaddr>,
    pub next_epoch_consensus_address: Option<Multiaddr>,
    pub next_epoch_worker_address: Option<Multiaddr>,
}

impl ValidatorMetadata {
    /// Verify validator metadata and return a verified version (on success) or error code (on failure)
    pub fn verify(&self) -> Result<VerifiedValidatorMetadata, u64> {
        // TODO: move the proof of possession verification here

        let protocol_pubkey =
            narwhal_crypto::PublicKey::from_bytes(self.protocol_pubkey_bytes.as_ref())
                .map_err(|_| E_METADATA_INVALID_PUBKEY)?;
        let network_pubkey =
            narwhal_crypto::NetworkPublicKey::from_bytes(self.network_pubkey_bytes.as_ref())
                .map_err(|_| E_METADATA_INVALID_NET_PUBKEY)?;
        let worker_pubkey =
            narwhal_crypto::NetworkPublicKey::from_bytes(self.worker_pubkey_bytes.as_ref())
                .map_err(|_| E_METADATA_INVALID_WORKER_PUBKEY)?;
        let net_address = Multiaddr::try_from(self.net_address.clone())
            .map_err(|_| E_METADATA_INVALID_NET_ADDR)?;
        let p2p_address = Multiaddr::try_from(self.p2p_address.clone())
            .map_err(|_| E_METADATA_INVALID_P2P_ADDR)?;
        let consensus_address = Multiaddr::try_from(self.consensus_address.clone())
            .map_err(|_| E_METADATA_INVALID_CONSENSUS_ADDR)?;
        let worker_address = Multiaddr::try_from(self.worker_address.clone())
            .map_err(|_| E_METADATA_INVALID_WORKER_ADDR)?;

        let next_epoch_protocol_pubkey = match self.next_epoch_protocol_pubkey_bytes.clone() {
            None => Ok::<Option<narwhal_crypto::PublicKey>, u64>(None),
            Some(bytes) => Ok(Some(
                narwhal_crypto::PublicKey::from_bytes(bytes.as_ref())
                    .map_err(|_| E_METADATA_INVALID_PUBKEY)?,
            )),
        }?;

        let next_epoch_network_pubkey = match self.next_epoch_network_pubkey_bytes.clone() {
            None => Ok::<Option<narwhal_crypto::NetworkPublicKey>, u64>(None),
            Some(bytes) => Ok(Some(
                narwhal_crypto::NetworkPublicKey::from_bytes(bytes.as_ref())
                    .map_err(|_| E_METADATA_INVALID_NET_PUBKEY)?,
            )),
        }?;

        let next_epoch_worker_pubkey: Option<narwhal_crypto::NetworkPublicKey> =
            match self.next_epoch_worker_pubkey_bytes.clone() {
                None => Ok::<Option<narwhal_crypto::NetworkPublicKey>, u64>(None),
                Some(bytes) => Ok(Some(
                    narwhal_crypto::NetworkPublicKey::from_bytes(bytes.as_ref())
                        .map_err(|_| E_METADATA_INVALID_WORKER_PUBKEY)?,
                )),
            }?;

        let next_epoch_net_address = match self.next_epoch_net_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_NET_ADDR)?,
            )),
        }?;

        let next_epoch_p2p_address = match self.next_epoch_p2p_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_P2P_ADDR)?,
            )),
        }?;

        let next_epoch_consensus_address = match self.next_epoch_consensus_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_CONSENSUS_ADDR)?,
            )),
        }?;

        let next_epoch_worker_address = match self.next_epoch_worker_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_WORKER_ADDR)?,
            )),
        }?;

        Ok(VerifiedValidatorMetadata {
            sui_address: self.sui_address,
            protocol_pubkey,
            network_pubkey,
            worker_pubkey,
            proof_of_possession_bytes: self.proof_of_possession_bytes.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            image_url: self.image_url.clone(),
            project_url: self.project_url.clone(),
            net_address,
            p2p_address,
            consensus_address,
            worker_address,
            next_epoch_protocol_pubkey,
            next_epoch_proof_of_possession: self.next_epoch_proof_of_possession.clone(),
            next_epoch_network_pubkey,
            next_epoch_worker_pubkey,
            next_epoch_net_address,
            next_epoch_p2p_address,
            next_epoch_consensus_address,
            next_epoch_worker_address,
        })
    }
}

impl ValidatorMetadata {
    pub fn network_address(&self) -> Result<Multiaddr> {
        Multiaddr::try_from(self.net_address.clone()).map_err(Into::into)
    }

    pub fn p2p_address(&self) -> Result<Multiaddr> {
        Multiaddr::try_from(self.p2p_address.clone()).map_err(Into::into)
    }

    pub fn narwhal_primary_address(&self) -> Result<Multiaddr> {
        Multiaddr::try_from(self.consensus_address.clone()).map_err(Into::into)
    }

    pub fn narwhal_worker_address(&self) -> Result<Multiaddr> {
        Multiaddr::try_from(self.worker_address.clone()).map_err(Into::into)
    }

    pub fn protocol_key(&self) -> AuthorityPublicKeyBytes {
        AuthorityPublicKeyBytes::from_bytes(self.protocol_pubkey_bytes.as_ref())
            .expect("Validity of public key bytes should be verified on-chain")
    }

    pub fn worker_key(&self) -> crate::crypto::NetworkPublicKey {
        crate::crypto::NetworkPublicKey::from_bytes(self.worker_pubkey_bytes.as_ref())
            .expect("Validity of public key bytes should be verified on-chain")
    }

    pub fn network_key(&self) -> crate::crypto::NetworkPublicKey {
        crate::crypto::NetworkPublicKey::from_bytes(self.network_pubkey_bytes.as_ref())
            .expect("Validity of public key bytes should be verified on-chain")
    }
}

/// Rust version of the Move sui::validator::Validator type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Validator {
    pub metadata: ValidatorMetadata,
    pub voting_power: u64,
    pub gas_price: u64,
    pub staking_pool: StakingPool,
    pub commission_rate: u64,
    pub next_epoch_stake: u64,
    pub next_epoch_gas_price: u64,
    pub next_epoch_commission_rate: u64,
}

impl Validator {
    pub fn to_current_epoch_committee_with_net_addresses(
        &self,
    ) -> (AuthorityName, StakeUnit, Vec<u8>) {
        (
            // TODO: Make sure we are actually verifying this on-chain.
            AuthorityPublicKeyBytes::from_bytes(self.metadata.protocol_pubkey_bytes.as_ref())
                .expect("Validity of public key bytes should be verified on-chain"),
            self.voting_power,
            self.metadata.net_address.clone(),
        )
    }

    pub fn authority_name(&self) -> AuthorityName {
        AuthorityPublicKeyBytes::from_bytes(self.metadata.protocol_pubkey_bytes.as_ref())
            .expect("Validity of public key bytes should be verified on-chain")
    }
}

/// Rust version of the Move sui::staking_pool::PendingDelegationEntry type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct PendingDelegationEntry {
    pub delegator: SuiAddress,
    pub sui_amount: u64,
    pub staked_sui_id: ObjectID,
}

/// Rust version of the Move sui::staking_pool::PendingWithdrawEntry type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct PendingWithdrawEntry {
    delegator: SuiAddress,
    principal_withdraw_amount: u64,
    withdrawn_pool_tokens: Balance,
}

/// Rust version of the Move sui::table::Table type. Putting it here since
/// we only use it in sui_system in the framework.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct TableVec {
    pub contents: Table,
}

impl Default for TableVec {
    fn default() -> Self {
        TableVec {
            contents: Table {
                id: ObjectID::from(SuiAddress::ZERO),
                size: 0,
            },
        }
    }
}

/// Rust version of the Move sui::table::Table type. Putting it here since
/// we only use it in sui_system in the framework.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct Table {
    pub id: ObjectID,
    pub size: u64,
}

impl Default for Table {
    fn default() -> Self {
        Table {
            id: ObjectID::from(SuiAddress::ZERO),
            size: 0,
        }
    }
}

/// Rust version of the Move sui::linked_table::LinkedTable type. Putting it here since
/// we only use it in sui_system in the framework.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct LinkedTable<K> {
    pub id: ObjectID,
    pub size: u64,
    pub head: MoveOption<K>,
    pub tail: MoveOption<K>,
}

impl<K> Default for LinkedTable<K> {
    fn default() -> Self {
        LinkedTable {
            id: ObjectID::from(SuiAddress::ZERO),
            size: 0,
            head: MoveOption { vec: vec![] },
            tail: MoveOption { vec: vec![] },
        }
    }
}

/// Rust version of the Move sui::staking_pool::StakingPool type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct StakingPool {
    pub id: ObjectID,
    pub starting_epoch: u64,
    pub sui_balance: u64,
    pub rewards_pool: Balance,
    pub pool_token_balance: u64,
    pub exchange_rates: Table,
    pub pending_delegation: u64,
    pub pending_total_sui_withdraw: u64,
    pub pending_pool_token_withdraw: u64,
}

/// Rust version of the Move sui::validator_set::ValidatorPair type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct ValidatorPair {
    from: SuiAddress,
    to: SuiAddress,
}

/// Rust version of the Move sui::validator_set::ValidatorSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct ValidatorSet {
    pub total_stake: u64,
    pub active_validators: Vec<Validator>,
    pub pending_validators: TableVec,
    pub pending_removals: Vec<u64>,
    pub staking_pool_mappings: Table,
}

/// Rust version of the Move sui::sui_system::SuiSystemStateInner type
/// We want to keep it named as SuiSystemState in Rust since this is the primary interface type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SuiSystemState {
    pub epoch: u64,
    pub protocol_version: u64,
    pub validators: ValidatorSet,
    pub storage_fund: Balance,
    pub parameters: SystemParameters,
    pub reference_gas_price: u64,
    pub validator_report_records: VecMap<SuiAddress, VecSet<SuiAddress>>,
    pub stake_subsidy: StakeSubsidy,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
    // TODO: Use getters instead of all pub.
}

/// Rust version of the Move sui::sui_system::SuiSystemState type
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuiSystemStateWrapper {
    pub id: UID,
    pub version: u64,
}

impl SuiSystemStateWrapper {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: SUI_SYSTEM_STATE_WRAPPER_STRUCT_NAME.to_owned(),
            module: SUI_SYSTEM_MODULE_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct StakeSubsidy {
    pub epoch_counter: u64,
    pub balance: Balance,
    pub current_epoch_amount: u64,
}

impl SuiSystemState {
    pub fn get_current_epoch_committee(&self) -> CommitteeWithNetAddresses {
        let mut voting_rights = BTreeMap::new();
        let mut net_addresses = BTreeMap::new();
        for validator in &self.validators.active_validators {
            let (name, voting_stake, net_address) =
                validator.to_current_epoch_committee_with_net_addresses();
            voting_rights.insert(name, voting_stake);
            net_addresses.insert(name, net_address);
        }
        CommitteeWithNetAddresses {
            committee: Committee::new(
                self.epoch,
                ProtocolVersion::new(self.protocol_version),
                voting_rights,
            )
            // unwrap is safe because we should have verified the committee on-chain.
            // TODO: Make sure we actually verify it.
            .unwrap(),
            net_addresses,
        }
    }

    #[allow(clippy::mutable_key_type)]
    pub fn get_current_epoch_narwhal_committee(&self) -> NarwhalCommittee {
        let narwhal_committee = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let verified_metadata = validator
                    .metadata
                    .verify()
                    .expect("Metadata should have been verified upon request");
                let authority = narwhal_config::Authority {
                    stake: validator.voting_power as narwhal_config::Stake,
                    primary_address: verified_metadata.consensus_address,
                    network_key: verified_metadata.network_pubkey,
                };
                (verified_metadata.protocol_pubkey, authority)
            })
            .collect();

        narwhal_config::Committee {
            authorities: narwhal_committee,
            epoch: self.epoch as narwhal_config::Epoch,
        }
    }

    pub fn get_current_epoch_authority_names_to_peer_ids(&self) -> HashMap<AuthorityName, PeerId> {
        let mut result = HashMap::new();
        let _: () = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let name = validator.authority_name();

                let network_key = narwhal_crypto::NetworkPublicKey::from_bytes(
                    &validator.metadata.network_pubkey_bytes,
                )
                .expect("Can't get narwhal network key");

                let peer_id = PeerId(network_key.0.to_bytes());

                result.insert(name, peer_id);
            })
            .collect();

        result
    }

    #[allow(clippy::mutable_key_type)]
    pub fn get_current_epoch_narwhal_worker_cache(
        &self,
        transactions_address: &Multiaddr,
    ) -> WorkerCache {
        let workers: BTreeMap<narwhal_crypto::PublicKey, WorkerIndex> = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let verified_metadata = validator
                    .metadata
                    .verify()
                    .expect("Metadata should have been verified upon request");
                let workers = [(
                    0,
                    narwhal_config::WorkerInfo {
                        name: verified_metadata.worker_pubkey,
                        transactions: transactions_address.clone(),
                        worker_address: verified_metadata.worker_address,
                    },
                )]
                .into_iter()
                .collect();
                let worker_index = WorkerIndex(workers);

                (verified_metadata.protocol_pubkey, worker_index)
            })
            .collect();
        WorkerCache {
            workers,
            epoch: self.epoch,
        }
    }
}

// The default implementation for tests
impl Default for SuiSystemState {
    fn default() -> Self {
        let validator_set = ValidatorSet {
            total_stake: 2,
            active_validators: vec![],
            pending_validators: TableVec::default(),
            pending_removals: vec![],
            staking_pool_mappings: Table::default(),
        };
        SuiSystemState {
            epoch: 0,
            protocol_version: ProtocolVersion::MIN.as_u64(),
            validators: validator_set,
            storage_fund: Balance::new(0),
            parameters: SystemParameters {
                min_validator_stake: 1,
                max_validator_candidate_count: 100,
            },
            reference_gas_price: 1,
            validator_report_records: VecMap { contents: vec![] },
            stake_subsidy: StakeSubsidy {
                epoch_counter: 0,
                balance: Balance::new(0),
                current_epoch_amount: 0,
            },
            safe_mode: false,
            epoch_start_timestamp_ms: 0,
        }
    }
}

pub fn get_sui_system_state_wrapper<S>(object_store: &S) -> Result<SuiSystemStateWrapper, SuiError>
where
    S: ObjectStore,
{
    let sui_system_object = object_store
        .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)?
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let move_object = sui_system_object
        .data
        .try_as_move()
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let result = bcs::from_bytes::<SuiSystemStateWrapper>(move_object.contents())
        .expect("Sui System State object deserialization cannot fail");
    Ok(result)
}

pub fn get_sui_system_state<S>(object_store: S) -> Result<SuiSystemState, SuiError>
where
    S: ObjectStore,
{
    let wrapper = get_sui_system_state_wrapper(&object_store)?;
    let inner_id = derive_dynamic_field_id(
        wrapper.id.id.bytes,
        &TypeTag::U64,
        &MoveTypeLayout::U64,
        &Value::u64(wrapper.version),
    )
    .expect("Sui System State object must exist");
    let inner = object_store
        .get_object(&inner_id)?
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let move_object = inner
        .data
        .try_as_move()
        .ok_or(SuiError::SuiSystemStateNotFound)?;
    let result = bcs::from_bytes::<Field<u64, SuiSystemState>>(move_object.contents())
        .expect("Sui System State object deserialization cannot fail");
    Ok(result.value)
}
