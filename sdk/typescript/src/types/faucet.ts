// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TransactionDigest, ObjectId } from './common';

export type FaucetCoinInfo = {
  amount: number;
  id: ObjectId;
  transfer_tx_digest: TransactionDigest;
};

export type FaucetResponse = {
  transferred_gas_objects: FaucetCoinInfo[];
  error: string | null;
};
