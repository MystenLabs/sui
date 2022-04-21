// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type as pick, string, Infer, number } from 'superstruct';

///////////////////////////////
// Exported Types
export type ObjectRef = Infer<typeof ObjectRef>;

export interface SignedTransaction {
  txBytes: string;
  signature: string;
  pubKey: string;
}

// TODO: use correct types here
export type TransactionResponse = string;

///////////////////////////////
// Exported Abstracts
export abstract class Provider {
  // Objects
  abstract getOwnedObjectRefs(address: string): Promise<ObjectRef[]>;

  // Transactions
  abstract executeTransaction(
    txn: SignedTransaction
  ): Promise<TransactionResponse>;

  // TODO: add more interface methods
}

export const ObjectRef = pick({
  digest: string(),
  objectId: string(),
  version: number(),
});
