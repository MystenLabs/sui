// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  Infer,
  number,
  object,
  string,
  tuple,
  boolean,
  optional,
  any,
  nullable,
} from 'superstruct';

import { TransactionDigest, TransactionEffectsDigest } from './common';

export const GasCostSummary = object({
  computationCost: string(),
  storageCost: string(),
  storageRebate: string(),
  nonRefundableStorageFee: string(),
});
export type GasCostSummary = Infer<typeof GasCostSummary>;

export const CheckPointContentsDigest = string();
export type CheckPointContentsDigest = Infer<typeof CheckPointContentsDigest>;

export const CheckpointDigest = string();
export type CheckpointDigest = Infer<typeof CheckpointDigest>;

export const ECMHLiveObjectSetDigest = object({
  digest: array(number()),
});
export type ECMHLiveObjectSetDigest = Infer<typeof ECMHLiveObjectSetDigest>;

export const CheckpointCommitment = any();
export type CheckpointCommitment = Infer<typeof CheckpointCommitment>;

export const ValidatorSignature = string();
export type ValidatorSignature = Infer<typeof ValidatorSignature>;

export const EndOfEpochData = object({
  nextEpochCommittee: array(tuple([string(), string()])),
  nextEpochProtocolVersion: string(),
  epochCommitments: array(CheckpointCommitment),
});
export type EndOfEpochData = Infer<typeof EndOfEpochData>;

export const ExecutionDigests = object({
  transaction: TransactionDigest,
  effects: TransactionEffectsDigest,
});

export const Checkpoint = object({
  epoch: string(),
  sequenceNumber: string(),
  digest: CheckpointDigest,
  networkTotalTransactions: string(),
  previousDigest: optional(CheckpointDigest),
  epochRollingGasCostSummary: GasCostSummary,
  timestampMs: string(),
  endOfEpochData: optional(EndOfEpochData),
  // TODO(jian): remove optional after 0.30.0 is released
  validatorSignature: optional(ValidatorSignature),
  transactions: array(TransactionDigest),
  checkpointCommitments: array(CheckpointCommitment),
});
export type Checkpoint = Infer<typeof Checkpoint>;

export const CheckpointPage = object({
  data: array(Checkpoint),
  nextCursor: nullable(string()),
  hasNextPage: boolean(),
});
export type CheckpointPage = Infer<typeof CheckpointPage>;
