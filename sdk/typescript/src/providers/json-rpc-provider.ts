// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Provider, SignedTransaction, TransactionResponse } from './provider';
import { JsonRpcClient } from '../rpc/client';
import { array, number, type as pick } from 'superstruct';
import {
  GetObjectInfoResponse,
  GetObjectInfoResponseSchema,
  ObjectRef,
  ObjectRefSchema,
} from '../types/objects';
import {
  CertifiedTransaction,
  CertifiedTransactionSchema,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  GetTxnDigestsResponseSchema,
  TransactionDigest,
} from '../types/transactions';

export class JsonRpcProvider extends Provider {
  private client: JsonRpcClient;

  /**
   * Establish a connection to a Sui Gateway endpoint
   *
   * @param endpoint URL to the Sui Gateway endpoint
   */
  constructor(endpoint: string) {
    super();
    this.client = new JsonRpcClient(endpoint);
  }

  // Objects
  async getOwnedObjectRefs(address: string): Promise<ObjectRef[]> {
    try {
      const resp = await this.client.requestWithValidation(
        'sui_getOwnedObjects',
        [address],
        pick({ objects: array(ObjectRefSchema) })
      );
      return resp.objects;
    } catch (err) {
      throw new Error(
        `Error fetching owned object refs: ${err} for address ${address}`
      );
    }
  }

  async getObjectInfo(objectId: string): Promise<GetObjectInfoResponse> {
    try {
      const resp = await this.client.requestWithValidation(
        'sui_getObjectTypedInfo',
        [objectId],
        GetObjectInfoResponseSchema
      );
      return resp;
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${objectId}`);
    }
  }

  // Transactions
  async getTransaction(
    digest: TransactionDigest
  ): Promise<CertifiedTransaction> {
    try {
      const resp = await this.client.requestWithValidation(
        'sui_getTransaction',
        [digest],
        CertifiedTransactionSchema
      );
      return resp;
    } catch (err) {
      throw new Error(`Error getting transaction: ${err} for digest ${digest}`);
    }
  }

  async executeTransaction(
    _txn: SignedTransaction
  ): Promise<TransactionResponse> {
    throw new Error('Method not implemented.');
  }

  async getTotalTransactionNumber(): Promise<number> {
    try {
      const resp = await this.client.requestWithValidation(
        'sui_getTotalTransactionNumber',
        [],
        number()
      );
      return resp;
    } catch (err) {
      throw new Error(`Error fetching total transaction number: ${err}`);
    }
  }

  async getTransactionDigestsInRange(
    start: GatewayTxSeqNumber,
    end: GatewayTxSeqNumber
  ): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithValidation(
        'sui_getTransactionsInRange',
        [start, end],
        GetTxnDigestsResponseSchema
      );
    } catch (err) {
      throw new Error(
        `Error fetching transaction digests in range: ${err} for range ${start}-${end}`
      );
    }
  }

  async getRecentTransactions(count: number): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithValidation(
        'sui_getRecentTransactions',
        [count],
        GetTxnDigestsResponseSchema
      );
    } catch (err) {
      throw new Error(
        `Error fetching recent transactions: ${err} for count ${count}`
      );
    }
  }

  // TODO: add more interface methods
}
