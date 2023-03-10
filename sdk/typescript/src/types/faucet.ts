// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionDigest, ObjectId } from './common';

export type FaucetCoinInfo = {
  amount: number;
  id: ObjectId;
  transferTxDigest: TransactionDigest;
};

export type FaucetResponse = {
  transferredGasObjects: FaucetCoinInfo[];
  error: string | null;
};
