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
  optional,
} from 'superstruct';

import { TransactionDigest, TransactionEffectsDigest } from './common';

export const GasCostSummary = object({
  computation_cost: number(),
  storage_cost: number(),
  storage_rebate: number(),
});
export type GasCostSummary = Infer<typeof GasCostSummary>;

export const CheckPointContentsDigest = string();
export type CheckPointContentsDigest = Infer<typeof CheckPointContentsDigest>;

export const CheckpointDigest = string();
export type CheckpointDigest = Infer<typeof CheckpointDigest>;

export const EndOfEpochData = object({
  next_epoch_committee: array(tuple([string(), number()])),
  next_epoch_protocol_version: number(),
  // Need to remove optional after we hit the next network version
  root_state_digest: optional(array(number())),
});
export type EndOfEpochData = Infer<typeof EndOfEpochData>;

export const ExecutionDigests = object({
  transaction: TransactionDigest,
  effects: TransactionEffectsDigest,
});

export const Checkpoint = object({
  epoch: number(),
  sequenceNumber: number(),
  digest: CheckpointDigest,
  networkTotalTransactions: number(),
  previousDigest: union([CheckpointDigest, literal(null)]),
  epochRollingGasCostSummary: GasCostSummary,
  timestampMs: union([number(), literal(null)]),
  endOfEpochData: union([EndOfEpochData, literal(null)]),
  transactions: array(TransactionDigest),
});
export type Checkpoint = Infer<typeof Checkpoint>;
