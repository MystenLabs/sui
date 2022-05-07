// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignedTransaction, TransactionResponse, Provider } from './provider';
import { JsonRpcClient } from '../rpc/client';
import {
  isGetObjectInfoResponse,
  isGetOwnedObjectRefsResponse,
  isGetTxnDigestsResponse,
  isTransactionEffectsResponse,
} from '../index.guard';
import {
  CertifiedTransaction,
  GatewayTxSeqNumber,
  GetTxnDigestsResponse,
  GetObjectInfoResponse,
  ObjectRef,
  TransactionDigest,
  TransactionEffectsResponse,
} from '../types';
import { transformGetObjectInfoResponse } from '../types/framework/transformer';

const isNumber = (val: any): val is number => typeof val === 'number';

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
      const resp = await this.client.requestWithType(
        'sui_getOwnedObjects',
        [address],
        isGetOwnedObjectRefsResponse
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
      const resp = await this.client.requestWithType(
        'sui_getObjectTypedInfo',
        [objectId],
        isGetObjectInfoResponse
      );
      return transformGetObjectInfoResponse(resp);
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${objectId}`);
    }
  }

  async getObjectInfoBatch(
    objectIds: string[]
  ): Promise<GetObjectInfoResponse[]> {
    const requests = objectIds.map(id => ({
      method: 'sui_getObjectTypedInfo',
      args: [id],
    }));
    try {
      const responses = await this.client.batchRequestWithType(
        requests,
        isGetObjectInfoResponse
      );
      return responses.map(r => transformGetObjectInfoResponse(r));
    } catch (err) {
      throw new Error(`Error fetching object info: ${err} for id ${objectIds}`);
    }
  }

  // Transactions
  async getTransactionWithEffects(
    digest: TransactionDigest
  ): Promise<TransactionEffectsResponse> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getTransaction',
        [digest],
        isTransactionEffectsResponse
      );
      return resp;
    } catch (err) {
      throw new Error(
        `Error getting transaction with effects: ${err} for digest ${digest}`
      );
    }
  }

  async getTransactionWithEffectsBatch(
    digests: TransactionDigest[]
  ): Promise<TransactionEffectsResponse[]> {
    const requests = digests.map(d => ({
      method: 'sui_getTransaction',
      args: [d],
    }));
    try {
      return await this.client.batchRequestWithType(
        requests,
        isTransactionEffectsResponse
      );
    } catch (err) {
      const list = digests.join(', ').substring(0, -2);
      throw new Error(`Error getting transaction effects: ${err} for digests [${list}]`);
    }
  }

  async getTransaction(
    digest: TransactionDigest
  ): Promise<CertifiedTransaction> {
    try {
      const resp = await this.client.requestWithType(
        'sui_getTransaction',
        [digest],
        isTransactionEffectsResponse
      );
      return resp.certificate;
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
      const resp = await this.client.requestWithType(
        'sui_getTotalTransactionNumber',
        [],
        isNumber
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
      return await this.client.requestWithType(
        'sui_getTransactionsInRange',
        [start, end],
        isGetTxnDigestsResponse
      );
    } catch (err) {
      throw new Error(
        `Error fetching transaction digests in range: ${err} for range ${start}-${end}`
      );
    }
  }

  async getRecentTransactions(count: number): Promise<GetTxnDigestsResponse> {
    try {
      return await this.client.requestWithType(
        'sui_getRecentTransactions',
        [count],
        isGetTxnDigestsResponse
      );
    } catch (err) {
      throw new Error(
        `Error fetching recent transactions: ${err} for count ${count}`
      );
    }
  }

  // TODO: add more interface methods
}
