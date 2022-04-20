// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  type as pick,
  string,
  Infer,
  number,
  enums,
  unknown,
  union,
} from 'superstruct';

///////////////////////////////
// Exported Types
export type ObjectRef = Infer<typeof ObjectRef>;
export type ObjectExistsInfo = Infer<typeof ObjectExistsInfo>;
export type ObjectNotExistsInfo = Infer<typeof ObjectNotExistsInfo>;
export type ObjectStatus = Infer<typeof ObjectStatus>;
export type GetObjectInfoResponse = Infer<typeof GetObjectInfoResponse>;

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
  /**
   * Get all objects owned by an address
   */
  abstract getOwnedObjectRefs(address: string): Promise<ObjectRef[]>;

  /**
   * Get information about an object
   */
  abstract getObjectInfo(objectId: string): Promise<GetObjectInfoResponse>;

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

export const ObjectExistsInfo = pick({
  objectRef: ObjectRef,
  object: unknown(),
});

export const ObjectNotExistsInfo = pick({
  objectId: string(),
});

export const ObjectStatus = enums(['Exists', 'NotExists', 'Deleted']);

export const GetObjectInfoResponse = pick({
  status: ObjectStatus,
  details: union([ObjectExistsInfo, ObjectNotExistsInfo, ObjectRef]),
});
