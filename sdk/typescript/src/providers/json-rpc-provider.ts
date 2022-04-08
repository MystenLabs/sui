// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  Provider,
  ObjectRef,
  SignedTransaction,
  TransactionResponse,
} from './provider';

export class JsonRpcProvider extends Provider {
  /** @internal */ _endpointURL: string;

  /**
   * Establish a connection to a Sui Gateway endpoint
   *
   * @param endpoint URL to the Sui Gateway endpoint
   */
  constructor(endpoint: string) {
    super();
    this._endpointURL = endpoint;
  }

  // Objects
  async getObjectRefs(_address: string): Promise<ObjectRef[]> {
    // TODO: implement the function with a RPC client
    return [];
  }

  // Transactions
  async executeTransaction(
    _txn: SignedTransaction
  ): Promise<TransactionResponse> {
    throw new Error('Method not implemented.');
  }

  // TODO: add more interface methods
}
