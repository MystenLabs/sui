// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  CertifiedTransaction,
  TransactionDigest,
  GetTxnDigestsResponse,
  GatewayTxSeqNumber,
  SuiObjectInfo,
  GetObjectDataResponse,
  TransactionResponse,
} from '../types';
import { Provider } from './provider';

export class VoidProvider extends Provider {
  // Objects
  async getObjectsOwnedByAddress(_address: string): Promise<SuiObjectInfo[]> {
    throw this.newError('getOwnedObjects');
  }

  async getObject(_objectId: string): Promise<GetObjectDataResponse> {
    throw this.newError('getObject');
  }

  // Transactions
  async getTransaction(
    _digest: TransactionDigest
  ): Promise<CertifiedTransaction> {
    throw this.newError('getTransaction');
  }

  async executeTransaction(
    _txnBytes: string,
    _signature: string,
    _pubkey: string
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
