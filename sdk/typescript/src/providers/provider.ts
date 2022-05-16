// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  GetObjectInfoResponse,
  SuiObjectRef,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  TransactionResponse,
} from '../types';

///////////////////////////////
// Exported Abstracts
export abstract class Provider {
  // Objects
  /**
   * Get all objects owned by an address
   */
  abstract getOwnedObjectRefs(address: string): Promise<SuiObjectRef[]>;

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
    txnBytes: string,
    signature: string,
    pubkey: string
  ): Promise<TransactionResponse>;

  // TODO: add more interface methods
}
