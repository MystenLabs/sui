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
  nullable,
  tuple,
  optional,
} from 'superstruct';
import { SuiAddress } from './common';
import { AuthorityName } from './transactions';

/* -------------- Types for the SuiSystemState Rust definition -------------- */

export const ValidatorMetaData = object({
  sui_address: SuiAddress,
  pubkey_bytes: array(number()),
  network_pubkey_bytes: array(number()),
  worker_pubkey_bytes: array(number()),
  proof_of_possession_bytes: array(number()),
  name: array(number()),
  description: nullable(array(any())),
  image_url: nullable(array(any())),
  project_url: nullable(array(any())),
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
export type CommitteeInfo = Infer<typeof CommitteeInfo>;

// Staking

export const Balance = object({
  value: number(),
});

export const StakedSui = object({
  id: object({
    id: string(),
  }),
  validator_address: SuiAddress,
  pool_starting_epoch: number(),
  delegation_request_epoch: number(),
  principal: Balance,
  sui_token_lock: union([number(), literal(null)]),
});

export const ActiveFields = object({
  id: object({
    id: string(),
  }),
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
  storage_gas_price: optional(string()),
});

export const Parameters = object({
  type: string(),
  fields: ParametersFields,
});

export const StakeSubsidyFields = object({
  balance: object({ value: number() }),
  current_epoch_amount: number(),
  epoch_counter: number(),
});

export const StakeSubsidy = object({
  type: string(),
  fields: StakeSubsidyFields,
});

export const SuiSupplyFields = object({
  value: number(),
});

export const ContentsFields = object({
  id: string(),
  size: number(),
  head: object({ vec: array() }),
  tail: object({ vec: array() }),
});

export const ContentsFieldsWithdraw = object({
  id: string(),
  size: number(),
});

export const Contents = object({
  type: string(),
  fields: ContentsFields,
});

export const PendingWithdawFields = object({
  contents: ContentsFieldsWithdraw,
});

export const DelegationStakingPoolFields = object({
  delegation_token_supply: SuiSupplyFields,
  pending_delegations: ContentsFields,
  pending_withdraws: PendingWithdawFields,
  rewards_pool: object({ value: number() }),
  starting_epoch: number(),
  sui_balance: number(),
  validator_address: string(),
});

export const DelegationStakingPool = object({
  type: string(),
  fields: DelegationStakingPoolFields,
});

export const CommitteeInfo = object({
  epoch: number(),
  // TODO(cleanup): remove optional after TestNet Wave 2(0.22.0)
  protocol_version: optional(number()),
  /* array of (validator public key, stake unit) tuple */
  committee_info: optional(array(tuple([AuthorityName, number()]))),
});

export const SystemParameters = object({
  min_validator_stake: number(),
  max_validator_candidate_count: number(),
  storage_gas_price: optional(number()),
});

export const Validator = object({
  metadata: ValidatorMetaData,
  voting_power: number(),
  stake_amount: number(),
  pending_stake: number(),
  pending_withdraw: number(),
  gas_price: number(),
  delegation_staking_pool: DelegationStakingPoolFields,
  commission_rate: number(),
});

export const ValidatorPair = object({
  from: SuiAddress,
  to: SuiAddress,
});

export const ValidatorSet = object({
  validator_stake: number(),
  delegation_stake: number(),
  active_validators: array(Validator),
  pending_validators: array(Validator),
  pending_removals: array(number()),
  next_epoch_validators: array(ValidatorMetaData),
  pending_delegation_switches: object({ contents: array(ValidatorPair) }),
});

export const SuiSystemState = object({
  info: object({ id: string() }),
  epoch: number(),
  // TODO(cleanup): remove optional after TestNet Wave 2(0.22.0)
  protocol_version: optional(number()),
  validators: ValidatorSet,
  treasury_cap: SuiSupplyFields,
  storage_fund: Balance,
  parameters: SystemParameters,
  reference_gas_price: number(),
  validator_report_records: object({ contents: array() }),
  stake_subsidy: StakeSubsidyFields,
  safe_mode: boolean(),
  epoch_start_timestamp_ms: optional(number()),
});

export type SuiSystemState = Infer<typeof SuiSystemState>;

/* ---------------- Types for the SuiSystemState Move object ---------------- */

const MovePendingWithdrawals = object({
  type: string(),
  fields: object({
    contents: object({
      type: string(),
      fields: object({
        id: object({
          id: string(),
        }),
        size: string(),
      }),
    }),
  }),
});

const MovePendingDelegations = object({
  type: string(),
  fields: object({
    id: object({
      id: string(),
    }),
    head: nullable(string()),
    size: string(),
    tail: nullable(string()),
  }),
});

export const MoveDelegationStakingPoolFields = object({
  delegation_token_supply: object({
    type: string(),
    fields: object({
      value: string(),
    }),
  }),
  pending_delegations: MovePendingDelegations,
  pending_withdraws: MovePendingWithdrawals,
  rewards_pool: string(),
  starting_epoch: string(),
  sui_balance: string(),
  validator_address: string(),
});

export type MoveSuiSystemObjectFields = Infer<typeof MoveSuiSystemObjectFields>;
export type MoveSuiSystemObject = Infer<typeof MoveSuiSystemObject>;
export type MoveActiveValidator = Infer<typeof MoveActiveValidator>;

const ValidatorReportRecords = object({
  type: string(),
  fields: object({
    contents: array(any()),
  }),
});

export const MoveNextEpochValidatorFields = object({
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

const MoveNextEpochValidator = object({
  type: string(),
  fields: MoveNextEpochValidatorFields,
});

export const MoveActiveValidatorFields = object({
  commission_rate: string(),
  delegation_staking_pool: object({
    type: string(),
    fields: MoveDelegationStakingPoolFields,
  }),
  gas_price: string(),
  metadata: MoveNextEpochValidator,
  pending_stake: string(),
  pending_withdraw: string(),
  stake_amount: string(),
  voting_power: nullable(string()),
});

export const MoveActiveValidator = object({
  type: string(),
  fields: MoveActiveValidatorFields,
});

export const MoveValidatorsFieldsClass = object({
  active_validators: array(MoveActiveValidator),
  next_epoch_validators: array(MoveNextEpochValidator),
  pending_delegation_switches: ValidatorReportRecords,
  pending_removals: array(number()),
  pending_validators: array(number()),
  quorum_stake_threshold: optional(string()),
  total_delegation_stake: string(),
  total_validator_stake: string(),
});

export const MoveSuiSystemObjectFields = object({
  chain_id: optional(number()),
  epoch: string(),
  // TODO(cleanup): remove optional after TestNet Wave 2(0.22.0)
  protocol_version: optional(string()),
  // TODO(cleanup): remove optional after TestNet Wave 2(0.22.0)
  epoch_start_timestamp_ms: optional(string()),
  safe_mode: boolean(),
  id: object({
    id: string(),
  }),
  parameters: Parameters,
  reference_gas_price: string(),
  stake_subsidy: object({
    type: string(),
    fields: object({
      balance: string(),
      current_epoch_amount: string(),
      epoch_counter: string(),
    }),
  }),
  storage_fund: string(),
  sui_supply: object({
    type: string(),
    fields: object({
      value: string(),
    }),
  }),
  validator_report_records: ValidatorReportRecords,
  validators: object({
    type: string(),
    fields: MoveValidatorsFieldsClass,
  }),
});

export const MoveSuiSystemObject = object({
  dataType: literal('moveObject'),
  type: string(),
  has_public_transfer: boolean(),
  fields: MoveSuiSystemObjectFields,
});
