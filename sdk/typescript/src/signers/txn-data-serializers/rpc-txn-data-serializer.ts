// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isTransactionBytes } from '../../index.guard';
import { JsonRpcClient } from '../../rpc/client';
import { Base64DataBuffer } from '../../serialization/base64';
import { SuiAddress } from '../../types';
import {
  MoveCallTransaction,
  MergeCoinTransaction,
  SplitCoinTransaction,
  TransferObjectTransaction,
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

  async newTransferObject(
    signerAddress: SuiAddress,
    t: TransferObjectTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const resp = await this.client.requestWithType(
        'sui_transferObject',
        [signerAddress, t.objectId, t.gasPayment, t.gasBudget, t.recipient],
        isTransactionBytes
      );
      return new Base64DataBuffer(resp.txBytes);
    } catch (err) {
      throw new Error(`Error transferring coin: ${err} with args ${t}`);
    }
  }

  async newMoveCall(
    signerAddress: SuiAddress,
    t: MoveCallTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const resp = await this.client.requestWithType(
        'sui_moveCall',
        [
          signerAddress,
          t.packageObjectId,
          t.module,
          t.function,
          t.typeArguments,
          t.arguments,
          t.gasPayment,
          t.gasBudget,
        ],
        isTransactionBytes
      );
      return new Base64DataBuffer(resp.txBytes);
    } catch (err) {
      throw new Error(`Error executing a move call: ${err} with args ${t}`);
    }
  }

  async newMergeCoin(
    signerAddress: SuiAddress,
    t: MergeCoinTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const resp = await this.client.requestWithType(
        'sui_mergeCoins',
        [
          signerAddress,
          t.primaryCoin,
          t.coinToMerge,
          t.gasPayment,
          t.gasBudget,
        ],
        isTransactionBytes
      );
      return new Base64DataBuffer(resp.txBytes);
    } catch (err) {
      throw new Error(`Error merging coin: ${err}`);
    }
  }

  async newSplitCoin(
    signerAddress: SuiAddress,
    t: SplitCoinTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const resp = await this.client.requestWithType(
        'sui_splitCoin',
        [
          signerAddress,
          t.coinObjectId,
          t.splitAmounts,
          t.gasPayment,
          t.gasBudget,
        ],
        isTransactionBytes
      );
      return new Base64DataBuffer(resp.txBytes);
    } catch (err) {
      throw new Error(`Error splitting coin: ${err}`);
    }
  }
}
