// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::balance::Balance;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::collection_types::VecMap;
use sui_types::committee::{EpochId, ProtocolVersion};
use sui_types::crypto::{
    get_key_pair, AuthorityPublicKeyBytes, KeypairTraits, NetworkKeyPair, ToFromBytes,
};
use sui_types::sui_system_state::SystemParameters;
use sui_types::sui_system_state::{
    StakeSubsidy, StakingPool, SuiSystemState, Table, TableVec, Validator, ValidatorMetadata,
    ValidatorSet,
};

pub fn test_validatdor_metadata(
    sui_address: SuiAddress,
    protocol_pubkey_bytes: AuthorityPublicKeyBytes,
    net_address: Vec<u8>,
) -> ValidatorMetadata {
    let network_keypair: NetworkKeyPair = get_key_pair().1;
    ValidatorMetadata {
        sui_address,
        protocol_pubkey_bytes: protocol_pubkey_bytes.as_bytes().to_vec(),
        network_pubkey_bytes: network_keypair.public().as_bytes().to_vec(),
        worker_pubkey_bytes: vec![],
        proof_of_possession_bytes: vec![],
        name: "zero_commission".to_string(),
        description: "".to_string(),
        image_url: "".to_string(),
        project_url: "".to_string(),
        net_address,
        p2p_address: vec![],
        primary_address: vec![],
        worker_address: vec![],
        next_epoch_protocol_pubkey_bytes: None,
        next_epoch_proof_of_possession: None,
        next_epoch_network_pubkey_bytes: None,
        next_epoch_worker_pubkey_bytes: None,
        next_epoch_net_address: None,
        next_epoch_p2p_address: None,
        next_epoch_primary_address: None,
        next_epoch_worker_address: None,
    }
}

pub fn test_staking_pool(sui_balance: u64) -> StakingPool {
    StakingPool {
        id: ObjectID::from(SuiAddress::ZERO),
        starting_epoch: 0,
        sui_balance,
        rewards_pool: Balance::new(0),
        pool_token_balance: 0,
        exchange_rates: Table::default(),
        pending_delegation: 0,
        pending_total_sui_withdraw: 0,
        pending_pool_token_withdraw: 0,
    }
}

pub fn test_validator(
    protocol_pubkey_bytes: AuthorityPublicKeyBytes,
    net_address: Vec<u8>,
    stake_amount: u64,
    delegated_amount: u64,
) -> Validator {
    let sui_address = SuiAddress::from(&protocol_pubkey_bytes);
    Validator {
        metadata: test_validatdor_metadata(sui_address, protocol_pubkey_bytes, net_address),
        voting_power: stake_amount + delegated_amount,
        gas_price: 1,
        staking_pool: test_staking_pool(delegated_amount + stake_amount),
        commission_rate: 0,
        next_epoch_stake: 1,
        next_epoch_gas_price: 1,
        next_epoch_commission_rate: 0,
    }
}

pub fn test_sui_system_state(epoch: EpochId, validators: Vec<Validator>) -> SuiSystemState {
    let validator_set = ValidatorSet {
        total_stake: 1,
        active_validators: validators,
        pending_validators: TableVec::default(),
        pending_removals: vec![],
        staking_pool_mappings: Table::default(),
    };
    SuiSystemState {
        epoch,
        protocol_version: ProtocolVersion::MAX.as_u64(),
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
