// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { array, nullable, number, object, Infer, string, union, literal } from 'superstruct';
import { TransactionDigest, ObjectId } from './common.js';

export const FaucetCoinInfo = object({
	amount: number(),
	id: ObjectId,
	transferTxDigest: TransactionDigest,
});

export type FaucetCoinInfo = Infer<typeof FaucetCoinInfo>;

export const FaucetResponse = object({
	transferredGasObjects: array(FaucetCoinInfo),
	error: nullable(string()),
});

export type FaucetResponse = Infer<typeof FaucetResponse>;

export const BatchFaucetResponse = object({
	task: nullable(string()),
	error: nullable(string()),
});

export type BatchFaucetResponse = Infer<typeof BatchFaucetResponse>;

export const BatchSendStatusType = union([
	literal('INPROGRESS'),
	literal('SUCCEEDED'),
	literal('DISCARDED'),
]);
export type BatchSendStatusType = Infer<typeof BatchSendStatusType>;

export const BatchStatusFaucetResponse = object({
	status: BatchSendStatusType,
	error: nullable(string()),
});

export type BatchStatusFaucetResponse = Infer<typeof BatchStatusFaucetResponse>;
