// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Infer } from 'superstruct';
import { array, boolean, nullable, number, object, optional, string } from 'superstruct';

export const CoinStruct = object({
	coinType: string(),
	// TODO(chris): rename this to objectId
	coinObjectId: string(),
	version: string(),
	digest: string(),
	balance: string(),
	previousTransaction: string(),
});

export type CoinStruct = Infer<typeof CoinStruct>;

export const PaginatedCoins = object({
	data: array(CoinStruct),
	nextCursor: nullable(string()),
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
