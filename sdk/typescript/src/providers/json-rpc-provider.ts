// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  Provider,
  ObjectRef,
  SignedTransaction,
  TransactionResponse,
} from './provider';
import { JsonRpcClient } from '../rpc/client';
import { array, type as pick } from 'superstruct';

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
        pick({ objects: array(ObjectRef) })
      );
      return resp.objects;
    } catch (err) {
      throw new Error(
        `Error fetching owned object refs: ${err} for address ${address}`
      );
    }
  }

  // Transactions
  async executeTransaction(
    _txn: SignedTransaction
  ): Promise<TransactionResponse> {
    throw new Error('Method not implemented.');
  }

  // TODO: add more interface methods
}
