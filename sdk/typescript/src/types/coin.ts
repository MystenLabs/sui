// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  Infer,
  literal,
  nullable,
  number,
  object,
  optional,
  string,
  union,
} from 'superstruct';
import { ObjectId, TransactionDigest } from './common';

export const CoinStruct = object({
  coinType: string(),
  coinObjectId: ObjectId,
  version: number(),
  digest: TransactionDigest,
  balance: number(),
  lockedUntilEpoch: nullable(number()),
  // TODO: remove optional when it is supported from all deployed networks
  previousTransaction: optional(TransactionDigest),
});

export type CoinStruct = Infer<typeof CoinStruct>;

export const PaginatedCoins = object({
  data: array(CoinStruct),
  nextCursor: union([ObjectId, literal(null)]),
});

export type PaginatedCoins = Infer<typeof PaginatedCoins>;

export const CoinBalance = object({
  coinType: string(),
  coinObjectCount: number(),
  totalBalance: number(),
  lockedBalance: object({
    epochId: optional(number()),
    number: optional(number()),
  }),
});

export type CoinBalance = Infer<typeof CoinBalance>;

export const CoinSupply = object({
  value: number(),
});

export type CoinSupply = Infer<typeof CoinSupply>;
