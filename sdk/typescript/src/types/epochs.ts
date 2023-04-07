// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { array, boolean, Infer, nullable, number, object } from 'superstruct';
import { SuiValidatorSummary } from './validator';

export const EndOfEpochInfo = object({
  lastCheckpointId: number(),
  epochEndTimestamp: number(),
  protocolVersion: number(),
  referenceGasPrice: number(),
  totalStake: number(),
  storageFundReinvestment: number(),
  storageCharge: number(),
  storageRebate: number(),
  storageFundBalance: number(),
  stakeSubsidyAmount: number(),
  totalGasFees: number(),
  totalStakeRewardsDistributed: number(),
  leftoverStorageFundInflow: number(),
});

export type EndOfEpochInfo = Infer<typeof EndOfEpochInfo>;

export const EpochInfo = object({
  epoch: number(),
  validators: array(SuiValidatorSummary),
  epochTotalTransactions: number(),
  firstCheckpointId: number(),
  epochStartTimestamp: number(),
  endOfEpochInfo: nullable(EndOfEpochInfo),
});

export type EpochInfo = Infer<typeof EpochInfo>;

export const EpochPage = object({
  data: array(EpochInfo),
  nextCursor: nullable(number()),
  hasNextPage: boolean(),
});

export type EpochPage = Infer<typeof EpochPage>;
