// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiMoveObject } from './objects';

export type Supply = {
  value: number;
};

export type SystemParameters = {
  max_validator_candidate_count: number;
  min_validator_stake: number;
  storage_gas_price: number;
};

export type PendingDelegationEntry = {
  delegator: string;
  sui_amount: number;
};

export type StakingPool = {
  starting_epoch: number;
  validator_address: string;
  sui_balance: number;
  rewards_pool: unknown;
  delegation_token_supply: unknown;
  pending_delegations: SuiMoveObject<PendingDelegationEntry>[];
  pending_withdraws: unknown;
};

export type ValidatorMetadata = {
  name: string;
  sui_address: string;
  pubkey_bytes: string;
  proof_of_possession: string;
  net_address: string;
  next_epoch_stake: number;
  next_epoch_delegation: number;
  next_epoch_gas_price: number;
  next_epoch_commission_rate: number;
};

export type Validator = {
  metadata: SuiMoveObject<ValidatorMetadata>;
  stake_amount: number;
  pending_stake: number;
  pending_withdraw: number;
  gas_price: number;
  commission_rate: number;
  delegation_staking_pool: SuiMoveObject<StakingPool>;
};

export type Validators = {
  active_validators: SuiMoveObject<Validator>[];
  pending_validators: SuiMoveObject<Validator>[];
  pending_removals: number[];
  next_epoch_validators: SuiMoveObject<ValidatorMetadata>[];
  total_validator_stake: number;
  total_delegation_stake: number;
  quorum_stake_threshold: number;
  pending_delegation_switches: unknown;
};

export type SuiSystemState = {
  id: { id: '0x0000000000000000000000000000000000000005' };
  epoch: number;
  parameters: SuiMoveObject<SystemParameters>;
  reference_gas_price: number;
  sui_supply: SuiMoveObject<Supply>;
  validators: SuiMoveObject<Validators>;
  validator_report_records: unknown;
};
