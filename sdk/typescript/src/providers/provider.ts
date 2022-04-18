// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

///////////////////////////////
// Exported Types
export interface ObjectRef {
  objectDigest: string;
  objectId: string;
  version: string;
}

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
  abstract getObjectRefs(address: string): Promise<ObjectRef[]>;

  // Transactions
  abstract executeTransaction(
    txn: SignedTransaction
  ): Promise<TransactionResponse>;

  // TODO: add more interface methods
}
