// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64 } from '@mysten/bcs';
import { JsonRpcClient } from '../../rpc/client';
import { isPureArg } from '../../types';
import { TransactionBytes } from '../../types/transactions';
import {
  MoveCallTransaction,
  MergeCoinTransaction,
  SplitCoinTransaction,
  TransferObjectTransaction,
  TransferSuiTransaction,
  PayTransaction,
  PaySuiTransaction,
  PayAllSuiTransaction,
  PublishTransaction,
  TxnDataSerializer,
  UnserializedSignableTransaction,
  TransactionBuilderMode,
} from './txn-data-serializer';

/**
 * This is a temporary implementation of the `TxnDataSerializer` class
 * that uses the Sui Fullnode RPC API to serialize a transaction into BCS bytes. We will
 * deprecate this implementation once `LocalTxnDataSerializer` stabilizes.
 *
 * Prefer to use `LocalTxnDataSerializer` instead for better performance and *security*, otherwise
 * this needs to be used with a trusted fullnode and it is recommended to verify the returned
 * BCS bytes matches the input.
 */
export class RpcTxnDataSerializer implements TxnDataSerializer {
  private client: JsonRpcClient;

  /**
   * Establish a connection to a Sui RPC endpoint
   *
   * @param endpoint URL to the Sui RPC endpoint
   * @param skipDataValidation default to `false`. If set to `true`, the rpc
   * client will not check if the responses from the RPC server conform to the schema
   * defined in the TypeScript SDK. The mismatches often happen when the SDK
   * is in a different version than the RPC server. Skipping the validation
   * can maximize the version compatibility of the SDK, as not all the schema
   * changes in the RPC response will affect the caller, but the caller needs to
   * understand that the data may not match the TypeSrcript definitions.
   */
  constructor(endpoint: string, private skipDataValidation: boolean = false) {
    this.client = new JsonRpcClient(endpoint);
  }

  async serializeToBytes(
    signerAddress: string,
    unserializedTxn: UnserializedSignableTransaction,
    mode: TransactionBuilderMode = 'Commit',
  ): Promise<Uint8Array> {
    let endpoint: string;
    let args: Array<any>;
    if (!unserializedTxn.data.gasBudget) {
      throw new Error('serializeToBytes requires a valid gas budget');
    }
    switch (unserializedTxn.kind) {
      case 'transferObject':
        const t = unserializedTxn.data as TransferObjectTransaction;
        endpoint = 'sui_transferObject';
        args = [
          signerAddress,
          t.objectId,
          t.gasPayment,
          t.gasBudget,
          t.recipient,
        ];
        break;
      case 'transferSui':
        const transferSui = unserializedTxn.data as TransferSuiTransaction;
        endpoint = 'sui_transferSui';
        args = [
          signerAddress,
          transferSui.suiObjectId,
          transferSui.gasBudget,
          transferSui.recipient,
          transferSui.amount,
        ];
        break;
      case 'pay':
        const pay = unserializedTxn.data as PayTransaction;
        endpoint = 'sui_pay';
        args = [
          signerAddress,
          pay.inputCoins,
          pay.recipients,
          pay.amounts,
          pay.gasPayment,
          pay.gasBudget,
        ];
        break;
      case 'paySui':
        const paySui = unserializedTxn.data as PaySuiTransaction;
        endpoint = 'sui_paySui';
        args = [
          signerAddress,
          paySui.inputCoins,
          paySui.recipients,
          paySui.amounts,
          paySui.gasBudget,
        ];
        break;
      case 'payAllSui':
        const payAllSui = unserializedTxn.data as PayAllSuiTransaction;
        endpoint = 'sui_payAllSui';
        args = [
          signerAddress,
          payAllSui.inputCoins,
          payAllSui.recipient,
          payAllSui.gasBudget,
        ];
        break;
      case 'moveCall':
        const moveCall = unserializedTxn.data as MoveCallTransaction;
        for (const arg of moveCall.arguments) {
          if (isPureArg(arg)) {
            throw new Error(
              'PureArg is not allowed as argument in RpcTxnDataSerializer. Please use LocalTxnDataSerializer instead.',
            );
          }
        }
        endpoint = 'sui_moveCall';
        args = [
          signerAddress,
          moveCall.packageObjectId,
          moveCall.module,
          moveCall.function,
          moveCall.typeArguments,
          moveCall.arguments,
          moveCall.gasPayment,
          moveCall.gasBudget,
          mode,
        ];
        break;
      case 'mergeCoin':
        const mergeCoin = unserializedTxn.data as MergeCoinTransaction;
        endpoint = 'sui_mergeCoins';
        args = [
          signerAddress,
          mergeCoin.primaryCoin,
          mergeCoin.coinToMerge,
          mergeCoin.gasPayment,
          mergeCoin.gasBudget,
        ];
        break;
      case 'splitCoin':
        const splitCoin = unserializedTxn.data as SplitCoinTransaction;
        endpoint = 'sui_splitCoin';
        args = [
          signerAddress,
          splitCoin.coinObjectId,
          splitCoin.splitAmounts,
          splitCoin.gasPayment,
          splitCoin.gasBudget,
        ];
        break;
      case 'publish':
        const publish = unserializedTxn.data as PublishTransaction;
        endpoint = 'sui_publish';
        args = [
          signerAddress,
          publish.compiledModules,
          publish.gasPayment,
          publish.gasBudget,
        ];
        break;
    }

    try {
      const resp = await this.client.requestWithType(
        endpoint,
        args,
        TransactionBytes,
        this.skipDataValidation,
      );
      return fromB64(resp.txBytes);
    } catch (e) {
      throw new Error(
        `Encountered error when calling RpcTxnDataSerialize for a ${unserializedTxn.kind} transaction for ` +
          `address ${signerAddress} for transaction ${JSON.stringify(
            unserializedTxn,
            null,
            2,
          )}: ${e}`,
      );
    }
  }
}
