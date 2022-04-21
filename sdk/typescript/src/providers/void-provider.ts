// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  Provider,
  ObjectRef,
  SignedTransaction,
  TransactionResponse,
  GetObjectInfoResponse,
  GetTxnDigestsResponse,
  GatewayTxSeqNumber,
} from './provider';

export class VoidProvider extends Provider {
  // Objects
  async getOwnedObjectRefs(_address: string): Promise<ObjectRef[]> {
    throw this.newError('getOwnedObjectRefs');
  }

  async getObjectInfo(_objectId: string): Promise<GetObjectInfoResponse> {
    throw this.newError('getObjectInfo');
  }

  // Transactions
  async executeTransaction(
    _txn: SignedTransaction
  ): Promise<TransactionResponse> {
    throw this.newError('executeTransaction');
  }

  async getTotalTransactionNumber(): Promise<number> {
    throw this.newError('getTotalTransactionNumber');
  }

  async getTransactionDigestsInRange(
    _start: GatewayTxSeqNumber,
    _end: GatewayTxSeqNumber
  ): Promise<GetTxnDigestsResponse> {
    throw this.newError('getTransactionDigestsInRange');
  }

  async getRecentTransactions(_count: number): Promise<GetTxnDigestsResponse> {
    throw this.newError('getRecentTransactions');
  }

  private newError(operation: string): Error {
    return new Error(`Please use a valid provider for ${operation}`);
  }
}
