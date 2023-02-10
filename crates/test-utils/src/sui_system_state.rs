// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bcs::to_bytes;
use sui_types::balance::{Balance, Supply};
use sui_types::base_types::SuiAddress;
use sui_types::collection_types::VecMap;
use sui_types::committee::{EpochId, ProtocolVersion};
use sui_types::crypto::{
    get_key_pair, AuthorityPublicKeyBytes, KeypairTraits, NetworkKeyPair, ToFromBytes,
};
use sui_types::id::UID;
use sui_types::sui_system_state::SystemParameters;
use sui_types::sui_system_state::{
    LinkedTable, StakeSubsidy, StakingPool, SuiSystemState, TableVec, Validator, ValidatorMetadata,
    ValidatorSet,
};
use sui_types::SUI_SYSTEM_STATE_OBJECT_ID;

pub fn test_validatdor_metadata(
    sui_address: SuiAddress,
    pubkey_bytes: AuthorityPublicKeyBytes,
    net_address: Vec<u8>,
) -> ValidatorMetadata {
    let network_keypair: NetworkKeyPair = get_key_pair().1;
    ValidatorMetadata {
        sui_address,
        pubkey_bytes: pubkey_bytes.as_bytes().to_vec(),
        network_pubkey_bytes: network_keypair.public().as_bytes().to_vec(),
        worker_pubkey_bytes: vec![],
        proof_of_possession_bytes: vec![],
        name: to_bytes("zero_commission").unwrap(),
        description: vec![],
        image_url: vec![],
        project_url: vec![],
        net_address,
        consensus_address: vec![],
        worker_address: vec![],
        next_epoch_stake: 1,
        next_epoch_delegation: 1,
        next_epoch_gas_price: 1,
        next_epoch_commission_rate: 0,
    }
}

pub fn test_staking_pool(sui_address: SuiAddress, sui_balance: u64) -> StakingPool {
    StakingPool {
        validator_address: sui_address,
        starting_epoch: 0,
        sui_balance,
        rewards_pool: Balance::new(0),
        delegation_token_supply: Supply { value: 0 },
        pending_delegations: LinkedTable::default(),
        pending_withdraws: TableVec::default(),
    }
}

pub fn test_validator(
    pubkey_bytes: AuthorityPublicKeyBytes,
    net_address: Vec<u8>,
    stake_amount: u64,
    delegated_amount: u64,
) -> Validator {
    let sui_address = SuiAddress::from(&pubkey_bytes);
    Validator {
        metadata: test_validatdor_metadata(sui_address, pubkey_bytes, net_address),
        voting_power: stake_amount,
        stake_amount,
        pending_stake: 1,
        pending_withdraw: 1,
        gas_price: 1,
        delegation_staking_pool: test_staking_pool(sui_address, delegated_amount),
        commission_rate: 0,
    }
}

pub fn test_sui_system_state(epoch: EpochId, validators: Vec<Validator>) -> SuiSystemState {
    let validator_set = ValidatorSet {
        validator_stake: 1,
        delegation_stake: 1,
        active_validators: validators,
        pending_validators: vec![],
        pending_removals: vec![],
        next_epoch_validators: vec![],
        pending_delegation_switches: VecMap { contents: vec![] },
    };
    SuiSystemState {
        info: UID::new(SUI_SYSTEM_STATE_OBJECT_ID),
        epoch,
        protocol_version: ProtocolVersion::MIN.0,
        validators: validator_set,
        treasury_cap: Supply { value: 0 },
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
