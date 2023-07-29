// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import {
	array,
	number,
	object,
	string,
	tuple,
	boolean,
	optional,
	any,
	nullable,
} from 'superstruct';

export const GasCostSummary = object({
	computationCost: string(),
	storageCost: string(),
	storageRebate: string(),
	nonRefundableStorageFee: string(),
});
export type GasCostSummary = Infer<typeof GasCostSummary>;

/** @deprecated Use `string` instead. */
export const CheckPointContentsDigest = string();
/** @deprecated Use `string` instead. */
export type CheckPointContentsDigest = Infer<typeof CheckPointContentsDigest>;

/** @deprecated Use `string` instead. */
export const CheckpointDigest = string();
/** @deprecated Use `string` instead. */
export type CheckpointDigest = Infer<typeof CheckpointDigest>;

export const ECMHLiveObjectSetDigest = object({
	digest: array(number()),
});
export type ECMHLiveObjectSetDigest = Infer<typeof ECMHLiveObjectSetDigest>;

export const CheckpointCommitment = any();
export type CheckpointCommitment = Infer<typeof CheckpointCommitment>;

/** @deprecated Use `string` instead. */
export const ValidatorSignature = string();
/** @deprecated Use `string` instead. */
export type ValidatorSignature = Infer<typeof ValidatorSignature>;

export const EndOfEpochData = object({
	nextEpochCommittee: array(tuple([string(), string()])),
	nextEpochProtocolVersion: string(),
	epochCommitments: array(CheckpointCommitment),
});
export type EndOfEpochData = Infer<typeof EndOfEpochData>;

export const ExecutionDigests = object({
	transaction: string(),
	effects: string(),
});

export const Checkpoint = object({
	epoch: string(),
	sequenceNumber: string(),
	digest: string(),
	networkTotalTransactions: string(),
	previousDigest: optional(string()),
	epochRollingGasCostSummary: GasCostSummary,
	timestampMs: string(),
	endOfEpochData: optional(EndOfEpochData),
	validatorSignature: string(),
	transactions: array(string()),
	checkpointCommitments: array(CheckpointCommitment),
});
export type Checkpoint = Infer<typeof Checkpoint>;

export const CheckpointPage = object({
	data: array(Checkpoint),
	nextCursor: nullable(string()),
	hasNextPage: boolean(),
});
export type CheckpointPage = Infer<typeof CheckpointPage>;
