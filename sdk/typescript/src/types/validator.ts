// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  any,
  array,
  boolean,
  literal,
  number,
  object,
  string,
  union,
  Infer,
  optional,
} from 'superstruct';
import { SuiAddress } from './common';

export const ValidatorMetaData = object({
  sui_address: SuiAddress,
  pubkey_bytes: array(number()),
  network_pubkey_bytes: array(number()),
  worker_pubkey_bytes: array(number()),
  proof_of_possession_bytes: array(number()),
  name: array(number()),
  description: optional(array(any())),
  image_url: optional(array(any())),
  project_url: optional(array(any())),
  net_address: array(number()),
  consensus_address: array(number()),
  worker_address: array(number()),
  next_epoch_stake: number(),
  next_epoch_delegation: number(),
  next_epoch_gas_price: number(),
  next_epoch_commission_rate: number(),
});

export type DelegatedStake = Infer<typeof DelegatedStake>;
export type ValidatorMetaData = Infer<typeof ValidatorMetaData>;
export type ValidatorsFields = Infer<typeof ValidatorsFields>;
export type Validators = Infer<typeof Validators>;
export type ActiveValidator = Infer<typeof ActiveValidator>;

// Staking
export const Id = object({
  id: string(),
});

export const Balance = object({
  value: number(),
});

export const StakedSui = object({
  id: Id,
  validator_address: SuiAddress,
  pool_starting_epoch: number(),
  delegation_request_epoch: number(),
  principal: Balance,
  sui_token_lock: union([number(), literal(null)]),
});

export const ID = object({
  id: string(),
});

export const ActiveFields = object({
  id: ID,
  staked_sui_id: SuiAddress,
  principal_sui_amount: number(),
  pool_tokens: Balance,
});

export const ActiveDelegationStatus = object({
  Active: ActiveFields,
});

export const DelegatedStake = object({
  staked_sui: StakedSui,
  delegation_status: union([literal('Pending'), ActiveDelegationStatus]),
});

export const ParametersFields = object({
  max_validator_candidate_count: string(),
  min_validator_stake: string(),
  storage_gas_price: string(),
});

export const Parameters = object({
  type: string(),
  fields: ParametersFields,
});

export const StakeSubsidyFields = object({
  balance: string(),
  current_epoch_amount: string(),
  epoch_counter: string(),
});

export const StakeSubsidy = object({
  type: string(),
  fields: StakeSubsidyFields,
});

export const SuiSupplyFields = object({
  value: string(),
});

export const Supply = object({
  type: string(),
  fields: SuiSupplyFields,
});

//TODO : add type for contents
export const ValidatorReportRecordsFields = object({
  contents: array(any()),
});

export const ValidatorReportRecords = object({
  type: string(),
  fields: ValidatorReportRecordsFields,
});

export const NextEpochValidatorFields = object({
  consensus_address: array(number()),
  name: union([string(), array(number())]),
  description: optional(union([string(), array(number())])),
  image_url: optional(union([string(), array(number())])),
  project_url: optional(union([string(), array(number())])),
  net_address: array(number()),
  network_pubkey_bytes: array(number()),
  next_epoch_commission_rate: string(),
  next_epoch_delegation: string(),
  next_epoch_gas_price: string(),
  next_epoch_stake: string(),
  proof_of_possession: array(number()),
  pubkey_bytes: array(number()),
  sui_address: string(),
  worker_address: array(number()),
  worker_pubkey_bytes: array(number()),
});

export const NextEpochValidator = object({
  type: string(),
  fields: NextEpochValidatorFields,
});

export const ContentsFields = object({
  id: ID,
  size: string(),
});

export const Contents = object({
  type: string(),
  fields: ContentsFields,
});

export const PendingDelegationsFields = object({
  contents: Contents,
});

export const Pending = object({
  type: string(),
  fields: PendingDelegationsFields,
});

export const DelegationStakingPoolFields = object({
  delegation_token_supply: Supply,
  pending_delegations: Pending,
  pending_withdraws: Pending,
  rewards_pool: string(),
  starting_epoch: string(),
  sui_balance: string(),
  validator_address: string(),
});

export const DelegationStakingPool = object({
  type: string(),
  fields: DelegationStakingPoolFields,
});

export const ActiveValidatorFields = object({
  commission_rate: string(),
  delegation_staking_pool: DelegationStakingPool,
  gas_price: string(),
  metadata: NextEpochValidator,
  pending_stake: string(),
  pending_withdraw: string(),
  stake_amount: string(),
});

export const ActiveValidator = object({
  type: string(),
  fields: ActiveValidatorFields,
});

export const ValidatorsFieldsClass = object({
  active_validators: array(ActiveValidator),
  next_epoch_validators: array(NextEpochValidator),
  pending_delegation_switches: ValidatorReportRecords,
  pending_removals: array(number()),
  pending_validators: array(number()),
  quorum_stake_threshold: string(),
  total_delegation_stake: string(),
  total_validator_stake: string(),
});

export const ValidatorsClass = object({
  type: string(),
  fields: ValidatorsFieldsClass,
});

export const ValidatorsFields = object({
  chain_id: number(),
  epoch: string(),
  id: Id,
  parameters: Parameters,
  reference_gas_price: string(),
  stake_subsidy: StakeSubsidy,
  storage_fund: string(),
  sui_supply: Supply,
  validator_report_records: ValidatorReportRecords,
  validators: ValidatorsClass,
});

export const Validators = object({
  dataType: string(),
  type: string(),
  has_public_transfer: boolean(),
  fields: ValidatorsFields,
});
