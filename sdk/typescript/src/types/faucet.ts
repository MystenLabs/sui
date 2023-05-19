// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { nullable, number, object, Infer, string } from 'superstruct';
import { TransactionDigest, ObjectId } from './common';

export const FaucetCoinInfo = object({
  amount: number(),
  id: ObjectId,
  transferTxDigest: TransactionDigest,
});

export type FaucetCoinInfo = Infer<typeof FaucetCoinInfo>;

export const FaucetResponse = object({
  task: nullable(string()),
  error: nullable(string()),
});

export type FaucetResponse = Infer<typeof FaucetResponse>;
