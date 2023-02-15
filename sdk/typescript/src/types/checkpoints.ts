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
});
export type EndOfEpochData = Infer<typeof EndOfEpochData>;

export const CheckpointSummary = object({
  epoch: number(),
  sequence_number: number(),
  network_total_transactions: number(),
  content_digest: CheckPointContentsDigest,
  previous_digest: union([CheckpointDigest, literal(null)]),
  epoch_rolling_gas_cost_summary: GasCostSummary,
  end_of_epoch_data: union([EndOfEpochData, literal(null)]),
  timestamp_ms: union([number(), literal(null)]),
  version_specific_data: optional(array(number())),
});
export type CheckpointSummary = Infer<typeof CheckpointSummary>;

export const ExecutionDigests = object({
  transaction: TransactionDigest,
  effects: TransactionEffectsDigest,
});

export const CheckpointContents = object({
  transactions: array(ExecutionDigests),
  user_signatures: array(string()),
});
export type CheckpointContents = Infer<typeof CheckpointContents>;
