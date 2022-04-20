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
  array,
  tuple,
} from 'superstruct';

///////////////////////////////
// Exported Types
export type ObjectRef = Infer<typeof ObjectRef>;
export type ObjectExistsInfo = Infer<typeof ObjectExistsInfo>;
export type ObjectNotExistsInfo = Infer<typeof ObjectNotExistsInfo>;
export type ObjectStatus = Infer<typeof ObjectStatus>;
export type GetObjectInfoResponse = Infer<typeof GetObjectInfoResponse>;
export type GatewayTxSeqNumber = Infer<typeof GatewayTxSeqNumber>;
export type TransactionDigest = Infer<typeof TransactionDigest>;
export type GetTransactionDigestInRange = Infer<
  typeof GetTransactionDigestInRange
>;

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
  /**
   * Get transaction digests for a given range
   *
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTransactionDigestsInRange(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber
  ): Promise<GetTransactionDigestInRange>;

  /**
   * Get total number of transactions
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getTotalTransactionNumber(): Promise<number>;

  abstract executeTransaction(
    txn: SignedTransaction
  ): Promise<TransactionResponse>;

  // TODO: add more interface methods
}

export const TransactionDigest = string();
export const GatewayTxSeqNumber = number();

export const ObjectRef = pick({
  digest: TransactionDigest,
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

export const GetTransactionDigestInRange = array(
  tuple([GatewayTxSeqNumber, TransactionDigest])
);
