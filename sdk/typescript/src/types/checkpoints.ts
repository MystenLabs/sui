// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  Infer,
  literal,
  number,
  object,
  string,
  union,
  tuple,
  boolean,
  optional,
  any,
} from 'superstruct';

import { TransactionDigest, TransactionEffectsDigest } from './common';

export const GasCostSummary = object({
  computationCost: number(),
  storageCost: number(),
  storageRebate: number(),
  nonRefundableStorageFee: number(),
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

export const EndOfEpochData = object({
  nextEpochCommittee: array(tuple([string(), number()])),
  nextEpochProtocolVersion: number(),
  epochCommitments: array(CheckpointCommitment),
});
export type EndOfEpochData = Infer<typeof EndOfEpochData>;

export const ExecutionDigests = object({
  transaction: TransactionDigest,
  effects: TransactionEffectsDigest,
});

export const Checkpoint = object({
  epoch: number(),
  sequenceNumber: string(),
  digest: CheckpointDigest,
  networkTotalTransactions: number(),
  previousDigest: optional(CheckpointDigest),
  epochRollingGasCostSummary: GasCostSummary,
  timestampMs: number(),
  endOfEpochData: optional(EndOfEpochData),
  transactions: array(TransactionDigest),
  checkpointCommitments: array(CheckpointCommitment),
});
export type Checkpoint = Infer<typeof Checkpoint>;

export const CheckpointPage = object({
  data: array(Checkpoint),
  nextCursor: union([string(), literal(null)]),
  hasNextPage: boolean(),
});
export type CheckpointPage = Infer<typeof CheckpointPage>;
