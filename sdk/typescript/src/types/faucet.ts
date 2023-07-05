// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { array, nullable, number, object, string } from 'superstruct';
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
