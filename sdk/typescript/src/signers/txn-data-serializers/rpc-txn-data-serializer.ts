// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isTransactionBytes } from '../../index.guard';
import { JsonRpcClient } from '../../rpc/client';
import { Base64DataBuffer } from '../../serialization/base64';
import {
  TransferCoinTransaction,
  TxnDataSerializer,
} from './txn-data-serializer';

/**
 * This is a temporary implementation of the `TxnDataSerializer` class
 * that uses the Sui Gateway RPC API to serialize a transaction into BCS bytes.
 * This class will be deprecated once we support BCS serialization in TypeScript.
 * It is not safe to use this class in production because one cannot authenticate
 * the encoding.
 */
export class RpcTxnDataSerializer implements TxnDataSerializer {
  private client: JsonRpcClient;

  /**
   * Establish a connection to a Sui Gateway endpoint
   *
   * @param endpoint URL to the Sui Gateway endpoint
   */
  constructor(endpoint: string) {
    this.client = new JsonRpcClient(endpoint);
  }

  async newTransferCoin(t: TransferCoinTransaction): Promise<Base64DataBuffer> {
    try {
      const resp = await this.client.requestWithType(
        'sui_transferCoin',
        [t.signer, t.objectId, t.gasPayment, t.gasBudget, t.recipient],
        isTransactionBytes
      );
      return new Base64DataBuffer(resp.tx_bytes);
    } catch (err) {
      throw new Error(`Error transferring coin: ${err}`);
    }
  }

  // TODO: add more interface methods
}
