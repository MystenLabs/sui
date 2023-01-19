// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { array, bigint, Infer, literal, map, number, object, optional, string, union } from 'superstruct';
import { ObjectId, TransactionDigest } from './common';

export const CoinStruct = object({
    coinType: string(),
    coinObjectId: union([ObjectId, literal(null)]),
    version: number(),
    digest: TransactionDigest,
    balance: number(),
    lockedUntilEpoch: optional(number()),
});
  
export type CoinStruct = Infer<typeof CoinStruct>;
  
export const PaginatedCoins = object({
    data: array(CoinStruct),
    nextCursor: union([ObjectId, literal(null)]),
});
  
export type PaginatedCoins = Infer<typeof PaginatedCoins>;

export const BalanceStruct = object({
    coinType: string(),
    coinObjectCount: number(),
    totalBalance: bigint(),
    lockedBalance: map(number(), bigint())
});

export type BalanceStruct = Infer<typeof BalanceStruct>;

export const SupplyStruct = object({
    value: number()
});

export type SupplyStruct = Infer<typeof SupplyStruct>;