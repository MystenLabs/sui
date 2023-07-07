// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { array, boolean, nullable, number, object, optional, string } from 'superstruct';
import { ObjectId, TransactionDigest } from './common.js';

export const CoinStruct = object({
	coinType: string(),
	// TODO(chris): rename this to objectId
	coinObjectId: ObjectId,
	version: string(),
	digest: TransactionDigest,
	balance: string(),
	previousTransaction: TransactionDigest,
});

export type CoinStruct = Infer<typeof CoinStruct>;

export const PaginatedCoins = object({
	data: array(CoinStruct),
	nextCursor: nullable(ObjectId),
	hasNextPage: boolean(),
});

export type PaginatedCoins = Infer<typeof PaginatedCoins>;

export const CoinBalance = object({
	coinType: string(),
	coinObjectCount: number(),
	totalBalance: string(),
	lockedBalance: object({
		epochId: optional(number()),
		number: optional(number()),
	}),
});

export type CoinBalance = Infer<typeof CoinBalance>;

export const CoinSupply = object({
	value: string(),
});

export type CoinSupply = Infer<typeof CoinSupply>;
