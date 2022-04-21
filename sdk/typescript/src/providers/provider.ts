// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0


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
  ): Promise<GetTxnDigestsResponse>;

  /**
   * Get the latest `count` transactions
   *
   * NOTE: this method may get deprecated after DevNet
   */
  abstract getRecentTransactions(count: number): Promise<GetTxnDigestsResponse>;

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

export type TransactionDigest = string;
export type GatewayTxSeqNumber = number;

export type ObjectRef = {
  digest: TransactionDigest,
  objectId: string,
  version: number,
};

export type ObjectExistsInfo = {
  objectRef: ObjectRef,
  object: any,
};

export type ObjectNotExistsInfo = {
  objectId: any,
};

export type ObjectStatus = 'Exists' | 'NotExists' | 'Deleted';

export type GetObjectInfoResponse = {
  status: ObjectStatus,
  details: ObjectExistsInfo | ObjectNotExistsInfo | ObjectRef,
};

export type GetOwnedObjectRefsResponse = {
  objects: ObjectRef[]
};

export type GetTxnDigestsResponse = [GatewayTxSeqNumber, TransactionDigest];
