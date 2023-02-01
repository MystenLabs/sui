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
  assert,
  nullable,
  tuple,
} from 'superstruct';
import { SuiAddress } from './common';
import { AuthorityName } from './transactions';

export const ValidatorMetaData = object({
  sui_address: SuiAddress,
  pubkey_bytes: array(number()),
  network_pubkey_bytes: array(number()),
  worker_pubkey_bytes: array(number()),
  proof_of_possession_bytes: array(number()),
  name: array(number()),
  description: array(array(any())),
  image_url: array(array(any())),
  project_url: array(array(any())),
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
export type CommitteeInfo = Infer<typeof CommitteeInfo>;

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
  description: nullable(union([string(), array(number())])),
  image_url: nullable(union([string(), array(number())])),
  project_url: nullable(union([string(), array(number())])),
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
  // id: ID,
  id: string(),
  // size: string(),
  size: number(),
  head: object({ vec: array() }),
  tail: object({ vec: array() }),
});

export const ContentsFieldsWithdraw = object({
  // id: ID,
  id: string(),
  // size: string(),
  size: number(),
});

export const Contents = object({
  type: string(),
  fields: ContentsFields,
});

export const PendingDelegationsFields = object({
  contents: ContentsFieldsWithdraw,
});

export const Pending = object({
  type: string(),
  fields: PendingDelegationsFields,
});

export const DelegationStakingPoolFields = object({
  delegation_token_supply: SuiSupplyFields,
  pending_delegations: ContentsFields,
  pending_withdraws: PendingDelegationsFields,
  rewards_pool: object({ value: number() }),
  // rewards_pool: string(),
  starting_epoch: number(),
  sui_balance: number(),
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
  voting_power: nullable(string()),
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

export const CommitteeInfo = object({
  epoch: number(),
  /* array of (validator public key, stake unit) tuple */
  committee_info: nullable(array(tuple([AuthorityName, number()]))),
});

export const SystemParameters = object({
  min_validator_stake: number(),
  max_validator_candidate_count: number(),
  // storage_gas_price: number(),
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
  // voting_power: number(),
  // quorum_threshold: number(),
  active_validators: array(Validator),
  pending_validators: array(Validator),
  pending_removals: array(number()),
  next_epoch_validators: array(ValidatorMetaData),
  pending_delegation_switches: object({ contents: array(ValidatorPair) }),
});

export const SuiSystemState = object({
  info: object({ id: string() }),
  // chain_id: number(),
  epoch: number(),
  validators: ValidatorSet,
  treasury_cap: SuiSupplyFields,
  storage_fund: Balance,
  parameters: SystemParameters,
  reference_gas_price: number(),
  validator_report_records: object({ contents: array() }),
  stake_subsidy: StakeSubsidyFields,
  safe_mode: boolean(),
  epoch_start_timestamp_ms: number(),
});

export type SuiSystemState = Infer<typeof SuiSystemState>;

assert(
  {
    info: { id: '0x0000000000000000000000000000000000000005' },
    epoch: 3,
    validators: {
      validator_stake: 400120012002964,
      delegation_stake: 0,
      active_validators: [
        {
          metadata: {
            sui_address: '0x14691bd5b9eb509033774d3d4c167dbef43094a7',
            pubkey_bytes: [
              138, 163, 157, 74, 52, 164, 68, 45, 196, 2, 53, 7, 188, 20, 27,
              45, 114, 225, 229, 96, 155, 57, 243, 154, 208, 89, 117, 184, 146,
              206, 245, 50, 54, 210, 241, 2, 74, 183, 167, 169, 211, 255, 100,
              89, 4, 222, 56, 87, 0, 57, 29, 204, 146, 136, 191, 136, 136, 162,
              139, 60, 218, 168, 188, 124, 160, 145, 206, 84, 22, 125, 207, 254,
              170, 240, 43, 208, 233, 129, 142, 3, 9, 54, 221, 168, 227, 116,
              21, 238, 59, 211, 92, 63, 214, 235, 107, 244,
            ],
            network_pubkey_bytes: [
              67, 67, 109, 112, 33, 31, 113, 82, 227, 176, 239, 248, 143, 215,
              164, 222, 46, 19, 78, 2, 46, 193, 95, 254, 132, 156, 97, 100, 135,
              56, 107, 5,
            ],
            worker_pubkey_bytes: [
              13, 144, 130, 97, 57, 108, 21, 5, 21, 180, 28, 188, 206, 86, 114,
              163, 158, 63, 60, 40, 151, 15, 103, 12, 109, 32, 75, 114, 5, 241,
              226, 158,
            ],
            proof_of_possession_bytes: [
              170, 90, 162, 192, 124, 75, 143, 148, 185, 125, 179, 167, 7, 37,
              96, 89, 69, 28, 97, 49, 96, 3, 78, 32, 7, 241, 193, 36, 81, 80,
              67, 158, 45, 176, 85, 197, 240, 226, 192, 138, 217, 3, 184, 221,
              216, 153, 108, 253,
            ],
            name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51],
            description: [],
            image_url: [],
            project_url: [],
            net_address: [4, 127, 0, 0, 1, 6, 148, 13, 224, 3],
            consensus_address: [4, 127, 0, 0, 1, 145, 2, 136, 113],
            worker_address: [4, 127, 0, 0, 1, 145, 2, 182, 133],
            next_epoch_stake: 100030003000741,
            next_epoch_delegation: 0,
            next_epoch_gas_price: 1,
            next_epoch_commission_rate: 0,
          },
          voting_power: 2500,
          stake_amount: 100030003000741,
          pending_stake: 0,
          pending_withdraw: 0,
          gas_price: 1,
          delegation_staking_pool: {
            validator_address: '0x14691bd5b9eb509033774d3d4c167dbef43094a7',
            starting_epoch: 1,
            sui_balance: 0,
            rewards_pool: { value: 0 },
            delegation_token_supply: { value: 0 },
            pending_delegations: {
              id: '0xd5d9aa879b78dc1f516d71ab979189086eff752f',
              size: 0,
              head: { vec: [] },
              tail: { vec: [] },
            },
            pending_withdraws: {
              contents: {
                id: '0xab8235dace3d68c7fb48110d63cbf4d6fd81ce10',
                size: 0,
              },
            },
          },
          commission_rate: 0,
        },
        {
          metadata: {
            sui_address: '0x01d71fb307dd0de5412c73b6d700ddf4aad7131a',
            pubkey_bytes: [
              167, 179, 241, 158, 163, 129, 150, 241, 59, 242, 143, 96, 50, 212,
              4, 124, 207, 68, 7, 222, 37, 169, 168, 230, 25, 116, 214, 231,
              138, 112, 9, 224, 146, 213, 182, 170, 35, 109, 91, 218, 1, 113,
              243, 82, 116, 97, 83, 197, 20, 5, 24, 16, 90, 152, 211, 254, 236,
              34, 234, 90, 130, 126, 90, 19, 87, 14, 103, 55, 110, 134, 74, 9,
              255, 185, 36, 145, 0, 239, 201, 232, 151, 101, 27, 214, 4, 28,
              182, 185, 143, 57, 152, 172, 213, 17, 185, 252,
            ],
            network_pubkey_bytes: [
              179, 80, 188, 213, 57, 21, 254, 254, 176, 116, 71, 204, 146, 42,
              240, 73, 42, 246, 205, 63, 244, 105, 216, 89, 198, 231, 103, 166,
              53, 81, 113, 198,
            ],
            worker_pubkey_bytes: [
              224, 155, 207, 32, 46, 170, 59, 232, 20, 232, 94, 108, 78, 175,
              54, 128, 176, 189, 149, 144, 236, 38, 46, 231, 136, 210, 2, 150,
              98, 195, 4, 8,
            ],
            proof_of_possession_bytes: [
              138, 84, 234, 179, 78, 189, 99, 20, 97, 131, 223, 248, 177, 170,
              60, 43, 166, 182, 149, 93, 145, 106, 214, 142, 77, 104, 255, 20,
              233, 11, 103, 51, 11, 108, 59, 174, 160, 71, 39, 220, 213, 223,
              99, 20, 185, 25, 135, 160,
            ],
            name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49],
            description: [],
            image_url: [],
            project_url: [],
            net_address: [4, 127, 0, 0, 1, 6, 149, 237, 224, 3],
            consensus_address: [4, 127, 0, 0, 1, 145, 2, 171, 11],
            worker_address: [4, 127, 0, 0, 1, 145, 2, 140, 123],
            next_epoch_stake: 100030003000741,
            next_epoch_delegation: 0,
            next_epoch_gas_price: 1,
            next_epoch_commission_rate: 0,
          },
          voting_power: 2500,
          stake_amount: 100030003000741,
          pending_stake: 0,
          pending_withdraw: 0,
          gas_price: 1,
          delegation_staking_pool: {
            validator_address: '0x01d71fb307dd0de5412c73b6d700ddf4aad7131a',
            starting_epoch: 1,
            sui_balance: 0,
            rewards_pool: { value: 0 },
            delegation_token_supply: { value: 0 },
            pending_delegations: {
              id: '0x628ffd0e51e9a6ea32c13c2739a31a8f344b557d',
              size: 0,
              head: { vec: [] },
              tail: { vec: [] },
            },
            pending_withdraws: {
              contents: {
                id: '0x1ace65f54d65a96251b3f46bfa720ab65a7ebe01',
                size: 0,
              },
            },
          },
          commission_rate: 0,
        },
        {
          metadata: {
            sui_address: '0x288251355ee03eef7ad7e0de158e31229e881612',
            pubkey_bytes: [
              169, 211, 35, 180, 9, 188, 17, 3, 200, 148, 150, 104, 246, 84, 47,
              103, 235, 209, 110, 37, 154, 33, 35, 202, 175, 154, 107, 0, 14,
              67, 162, 121, 208, 57, 21, 98, 233, 223, 110, 104, 175, 64, 10,
              90, 191, 120, 98, 203, 7, 203, 199, 230, 63, 110, 131, 51, 166,
              251, 142, 167, 34, 28, 28, 162, 221, 80, 111, 98, 118, 220, 4,
              100, 20, 85, 15, 230, 230, 158, 188, 43, 214, 129, 101, 113, 171,
              111, 83, 237, 56, 98, 198, 139, 4, 155, 67, 134,
            ],
            network_pubkey_bytes: [
              93, 13, 247, 215, 43, 252, 107, 89, 105, 221, 167, 206, 222, 146,
              204, 113, 177, 229, 63, 185, 39, 76, 210, 140, 99, 93, 102, 251,
              22, 192, 96, 188,
            ],
            worker_pubkey_bytes: [
              104, 244, 100, 226, 60, 124, 231, 75, 146, 158, 153, 173, 46, 191,
              169, 165, 103, 242, 101, 203, 146, 101, 13, 127, 76, 205, 36, 23,
              88, 79, 85, 27,
            ],
            proof_of_possession_bytes: [
              173, 145, 158, 30, 118, 42, 213, 250, 223, 22, 166, 132, 124, 161,
              68, 85, 240, 68, 255, 140, 14, 36, 106, 219, 17, 196, 28, 111,
              212, 160, 55, 73, 221, 22, 152, 32, 177, 71, 6, 121, 218, 198, 89,
              172, 215, 167, 71, 44,
            ],
            name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 48],
            description: [],
            image_url: [],
            project_url: [],
            net_address: [4, 127, 0, 0, 1, 6, 161, 71, 224, 3],
            consensus_address: [4, 127, 0, 0, 1, 145, 2, 149, 87],
            worker_address: [4, 127, 0, 0, 1, 145, 2, 152, 243],
            next_epoch_stake: 100030003000741,
            next_epoch_delegation: 0,
            next_epoch_gas_price: 1,
            next_epoch_commission_rate: 0,
          },
          voting_power: 2500,
          stake_amount: 100030003000741,
          pending_stake: 0,
          pending_withdraw: 0,
          gas_price: 1,
          delegation_staking_pool: {
            validator_address: '0x288251355ee03eef7ad7e0de158e31229e881612',
            starting_epoch: 1,
            sui_balance: 0,
            rewards_pool: { value: 0 },
            delegation_token_supply: { value: 0 },
            pending_delegations: {
              id: '0x6d3ffc5213ed4df6802cd4535d3c18f66d85bab5',
              size: 0,
              head: { vec: [] },
              tail: { vec: [] },
            },
            pending_withdraws: {
              contents: {
                id: '0x2cd564ff647db701afe7b1e8a3f1a31bc071fd0b',
                size: 0,
              },
            },
          },
          commission_rate: 0,
        },
        {
          metadata: {
            sui_address: '0x51c34585413d4d1c320fff0dce8a485f207166fe',
            pubkey_bytes: [
              173, 76, 117, 232, 59, 214, 244, 143, 123, 89, 190, 76, 148, 253,
              99, 254, 248, 79, 196, 33, 50, 246, 36, 203, 167, 198, 71, 95,
              205, 58, 81, 51, 84, 68, 211, 87, 43, 199, 254, 160, 73, 24, 70,
              96, 52, 43, 232, 72, 17, 64, 59, 16, 255, 249, 216, 93, 125, 124,
              182, 185, 145, 127, 250, 235, 131, 77, 250, 36, 167, 28, 112, 2,
              107, 41, 50, 232, 70, 162, 30, 38, 23, 73, 114, 21, 5, 183, 133,
              29, 160, 197, 185, 183, 195, 207, 9, 157,
            ],
            network_pubkey_bytes: [
              233, 242, 221, 109, 203, 25, 170, 99, 39, 242, 165, 169, 100, 56,
              197, 189, 60, 223, 204, 4, 122, 220, 245, 116, 25, 15, 236, 89,
              175, 135, 229, 251,
            ],
            worker_pubkey_bytes: [
              217, 241, 18, 207, 20, 183, 127, 185, 9, 68, 161, 140, 105, 249,
              55, 229, 4, 136, 182, 244, 186, 186, 101, 228, 18, 185, 176, 37,
              234, 99, 45, 95,
            ],
            proof_of_possession_bytes: [
              183, 32, 92, 33, 164, 66, 30, 245, 153, 75, 149, 119, 190, 6, 200,
              175, 44, 191, 228, 182, 167, 6, 246, 242, 154, 130, 190, 102, 71,
              118, 109, 160, 8, 145, 54, 161, 26, 212, 54, 216, 153, 220, 102,
              255, 210, 11, 113, 181,
            ],
            name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50],
            description: [],
            image_url: [],
            project_url: [],
            net_address: [4, 127, 0, 0, 1, 6, 149, 33, 224, 3],
            consensus_address: [4, 127, 0, 0, 1, 145, 2, 160, 3],
            worker_address: [4, 127, 0, 0, 1, 145, 2, 161, 111],
            next_epoch_stake: 100030003000741,
            next_epoch_delegation: 0,
            next_epoch_gas_price: 1,
            next_epoch_commission_rate: 0,
          },
          voting_power: 2500,
          stake_amount: 100030003000741,
          pending_stake: 0,
          pending_withdraw: 0,
          gas_price: 1,
          delegation_staking_pool: {
            validator_address: '0x51c34585413d4d1c320fff0dce8a485f207166fe',
            starting_epoch: 1,
            sui_balance: 0,
            rewards_pool: { value: 0 },
            delegation_token_supply: { value: 0 },
            pending_delegations: {
              id: '0x01b3b1dd18a3b775fe0e0d4b873c0aa0d0cd2acf',
              size: 0,
              head: { vec: [] },
              tail: { vec: [] },
            },
            pending_withdraws: {
              contents: {
                id: '0x3c2b307c3239f61643af5e9a09d7d0c95bfe14dc',
                size: 0,
              },
            },
          },
          commission_rate: 0,
        },
      ],
      pending_validators: [],
      pending_removals: [],
      next_epoch_validators: [
        {
          sui_address: '0x51c34585413d4d1c320fff0dce8a485f207166fe',
          pubkey_bytes: [
            173, 76, 117, 232, 59, 214, 244, 143, 123, 89, 190, 76, 148, 253,
            99, 254, 248, 79, 196, 33, 50, 246, 36, 203, 167, 198, 71, 95, 205,
            58, 81, 51, 84, 68, 211, 87, 43, 199, 254, 160, 73, 24, 70, 96, 52,
            43, 232, 72, 17, 64, 59, 16, 255, 249, 216, 93, 125, 124, 182, 185,
            145, 127, 250, 235, 131, 77, 250, 36, 167, 28, 112, 2, 107, 41, 50,
            232, 70, 162, 30, 38, 23, 73, 114, 21, 5, 183, 133, 29, 160, 197,
            185, 183, 195, 207, 9, 157,
          ],
          network_pubkey_bytes: [
            233, 242, 221, 109, 203, 25, 170, 99, 39, 242, 165, 169, 100, 56,
            197, 189, 60, 223, 204, 4, 122, 220, 245, 116, 25, 15, 236, 89, 175,
            135, 229, 251,
          ],
          worker_pubkey_bytes: [
            217, 241, 18, 207, 20, 183, 127, 185, 9, 68, 161, 140, 105, 249, 55,
            229, 4, 136, 182, 244, 186, 186, 101, 228, 18, 185, 176, 37, 234,
            99, 45, 95,
          ],
          proof_of_possession_bytes: [
            183, 32, 92, 33, 164, 66, 30, 245, 153, 75, 149, 119, 190, 6, 200,
            175, 44, 191, 228, 182, 167, 6, 246, 242, 154, 130, 190, 102, 71,
            118, 109, 160, 8, 145, 54, 161, 26, 212, 54, 216, 153, 220, 102,
            255, 210, 11, 113, 181,
          ],
          name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50],
          description: [],
          image_url: [],
          project_url: [],
          net_address: [4, 127, 0, 0, 1, 6, 149, 33, 224, 3],
          consensus_address: [4, 127, 0, 0, 1, 145, 2, 160, 3],
          worker_address: [4, 127, 0, 0, 1, 145, 2, 161, 111],
          next_epoch_stake: 100030003000741,
          next_epoch_delegation: 0,
          next_epoch_gas_price: 1,
          next_epoch_commission_rate: 0,
        },
        {
          sui_address: '0x288251355ee03eef7ad7e0de158e31229e881612',
          pubkey_bytes: [
            169, 211, 35, 180, 9, 188, 17, 3, 200, 148, 150, 104, 246, 84, 47,
            103, 235, 209, 110, 37, 154, 33, 35, 202, 175, 154, 107, 0, 14, 67,
            162, 121, 208, 57, 21, 98, 233, 223, 110, 104, 175, 64, 10, 90, 191,
            120, 98, 203, 7, 203, 199, 230, 63, 110, 131, 51, 166, 251, 142,
            167, 34, 28, 28, 162, 221, 80, 111, 98, 118, 220, 4, 100, 20, 85,
            15, 230, 230, 158, 188, 43, 214, 129, 101, 113, 171, 111, 83, 237,
            56, 98, 198, 139, 4, 155, 67, 134,
          ],
          network_pubkey_bytes: [
            93, 13, 247, 215, 43, 252, 107, 89, 105, 221, 167, 206, 222, 146,
            204, 113, 177, 229, 63, 185, 39, 76, 210, 140, 99, 93, 102, 251, 22,
            192, 96, 188,
          ],
          worker_pubkey_bytes: [
            104, 244, 100, 226, 60, 124, 231, 75, 146, 158, 153, 173, 46, 191,
            169, 165, 103, 242, 101, 203, 146, 101, 13, 127, 76, 205, 36, 23,
            88, 79, 85, 27,
          ],
          proof_of_possession_bytes: [
            173, 145, 158, 30, 118, 42, 213, 250, 223, 22, 166, 132, 124, 161,
            68, 85, 240, 68, 255, 140, 14, 36, 106, 219, 17, 196, 28, 111, 212,
            160, 55, 73, 221, 22, 152, 32, 177, 71, 6, 121, 218, 198, 89, 172,
            215, 167, 71, 44,
          ],
          name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 48],
          description: [],
          image_url: [],
          project_url: [],
          net_address: [4, 127, 0, 0, 1, 6, 161, 71, 224, 3],
          consensus_address: [4, 127, 0, 0, 1, 145, 2, 149, 87],
          worker_address: [4, 127, 0, 0, 1, 145, 2, 152, 243],
          next_epoch_stake: 100030003000741,
          next_epoch_delegation: 0,
          next_epoch_gas_price: 1,
          next_epoch_commission_rate: 0,
        },
        {
          sui_address: '0x01d71fb307dd0de5412c73b6d700ddf4aad7131a',
          pubkey_bytes: [
            167, 179, 241, 158, 163, 129, 150, 241, 59, 242, 143, 96, 50, 212,
            4, 124, 207, 68, 7, 222, 37, 169, 168, 230, 25, 116, 214, 231, 138,
            112, 9, 224, 146, 213, 182, 170, 35, 109, 91, 218, 1, 113, 243, 82,
            116, 97, 83, 197, 20, 5, 24, 16, 90, 152, 211, 254, 236, 34, 234,
            90, 130, 126, 90, 19, 87, 14, 103, 55, 110, 134, 74, 9, 255, 185,
            36, 145, 0, 239, 201, 232, 151, 101, 27, 214, 4, 28, 182, 185, 143,
            57, 152, 172, 213, 17, 185, 252,
          ],
          network_pubkey_bytes: [
            179, 80, 188, 213, 57, 21, 254, 254, 176, 116, 71, 204, 146, 42,
            240, 73, 42, 246, 205, 63, 244, 105, 216, 89, 198, 231, 103, 166,
            53, 81, 113, 198,
          ],
          worker_pubkey_bytes: [
            224, 155, 207, 32, 46, 170, 59, 232, 20, 232, 94, 108, 78, 175, 54,
            128, 176, 189, 149, 144, 236, 38, 46, 231, 136, 210, 2, 150, 98,
            195, 4, 8,
          ],
          proof_of_possession_bytes: [
            138, 84, 234, 179, 78, 189, 99, 20, 97, 131, 223, 248, 177, 170, 60,
            43, 166, 182, 149, 93, 145, 106, 214, 142, 77, 104, 255, 20, 233,
            11, 103, 51, 11, 108, 59, 174, 160, 71, 39, 220, 213, 223, 99, 20,
            185, 25, 135, 160,
          ],
          name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49],
          description: [],
          image_url: [],
          project_url: [],
          net_address: [4, 127, 0, 0, 1, 6, 149, 237, 224, 3],
          consensus_address: [4, 127, 0, 0, 1, 145, 2, 171, 11],
          worker_address: [4, 127, 0, 0, 1, 145, 2, 140, 123],
          next_epoch_stake: 100030003000741,
          next_epoch_delegation: 0,
          next_epoch_gas_price: 1,
          next_epoch_commission_rate: 0,
        },
        {
          sui_address: '0x14691bd5b9eb509033774d3d4c167dbef43094a7',
          pubkey_bytes: [
            138, 163, 157, 74, 52, 164, 68, 45, 196, 2, 53, 7, 188, 20, 27, 45,
            114, 225, 229, 96, 155, 57, 243, 154, 208, 89, 117, 184, 146, 206,
            245, 50, 54, 210, 241, 2, 74, 183, 167, 169, 211, 255, 100, 89, 4,
            222, 56, 87, 0, 57, 29, 204, 146, 136, 191, 136, 136, 162, 139, 60,
            218, 168, 188, 124, 160, 145, 206, 84, 22, 125, 207, 254, 170, 240,
            43, 208, 233, 129, 142, 3, 9, 54, 221, 168, 227, 116, 21, 238, 59,
            211, 92, 63, 214, 235, 107, 244,
          ],
          network_pubkey_bytes: [
            67, 67, 109, 112, 33, 31, 113, 82, 227, 176, 239, 248, 143, 215,
            164, 222, 46, 19, 78, 2, 46, 193, 95, 254, 132, 156, 97, 100, 135,
            56, 107, 5,
          ],
          worker_pubkey_bytes: [
            13, 144, 130, 97, 57, 108, 21, 5, 21, 180, 28, 188, 206, 86, 114,
            163, 158, 63, 60, 40, 151, 15, 103, 12, 109, 32, 75, 114, 5, 241,
            226, 158,
          ],
          proof_of_possession_bytes: [
            170, 90, 162, 192, 124, 75, 143, 148, 185, 125, 179, 167, 7, 37, 96,
            89, 69, 28, 97, 49, 96, 3, 78, 32, 7, 241, 193, 36, 81, 80, 67, 158,
            45, 176, 85, 197, 240, 226, 192, 138, 217, 3, 184, 221, 216, 153,
            108, 253,
          ],
          name: [118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51],
          description: [],
          image_url: [],
          project_url: [],
          net_address: [4, 127, 0, 0, 1, 6, 148, 13, 224, 3],
          consensus_address: [4, 127, 0, 0, 1, 145, 2, 136, 113],
          worker_address: [4, 127, 0, 0, 1, 145, 2, 182, 133],
          next_epoch_stake: 100030003000741,
          next_epoch_delegation: 0,
          next_epoch_gas_price: 1,
          next_epoch_commission_rate: 0,
        },
      ],
      pending_delegation_switches: { contents: [] },
    },
    treasury_cap: { value: 400120012000401 },
    storage_fund: { value: 1682 },
    parameters: { min_validator_stake: 1, max_validator_candidate_count: 100 },
    reference_gas_price: 1,
    validator_report_records: { contents: [] },
    stake_subsidy: {
      epoch_counter: 0,
      balance: { value: 0 },
      current_epoch_amount: 1000000,
    },
    safe_mode: false,
    epoch_start_timestamp_ms: 1674643178068,
  },
  SuiSystemState
);
