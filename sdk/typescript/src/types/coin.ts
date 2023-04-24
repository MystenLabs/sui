// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  array,
  boolean,
  Infer,
  nullable,
  number,
  object,
  optional,
  string,
} from 'superstruct';
import { ObjectId, TransactionDigest } from './common';

export const CoinStruct = object({
  coinType: string(),
  // TODO(chris): rename this to objectId
  coinObjectId: ObjectId,
  version: string(),
  digest: TransactionDigest,
  balance: string(),
  // TODO (jian): remove this when we move to 0.34
  lockedUntilEpoch: optional(nullable(number())),
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
