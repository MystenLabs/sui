// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';
import {
  CallArg,
  MoveCallTx,
  registerTypes,
  SuiAddress,
  TransactionData,
  TypeTag,
} from '../../types';
import { bcs as BCS } from '@mysten/bcs';
import {
  MoveCallTransaction,
  MergeCoinTransaction,
  SplitCoinTransaction,
  TransferObjectTransaction,
  TransferSuiTransaction,
  PublishTransaction,
  TxnDataSerializer,
} from './txn-data-serializer';
import { Provider } from '../../providers/provider';

// For commonjs scenario need to figure out how to do it differently
const bcs = registerTypes(BCS);
export { bcs };

export class LocalTxnDataSerializer implements TxnDataSerializer {
  /**
   * Need a provider to fetch the latest object reference. Ideally the provider
   * should cache the object reference locally
   */
  constructor(private provider: Provider) {}

  async newTransferObject(
    _signerAddress: SuiAddress,
    _t: TransferObjectTransaction
  ): Promise<Base64DataBuffer> {
    throw new Error('Not implemented');
  }

  async newTransferSui(
    _signerAddress: SuiAddress,
    _t: TransferSuiTransaction
  ): Promise<Base64DataBuffer> {
    throw new Error('Not implemented');
  }

  async newMoveCall(
    signerAddress: SuiAddress,
    t: MoveCallTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const pkg = await this.provider.getObjectRef(t.packageObjectId);
      const tx = {
        Call: {
          package: pkg!,
          module: t.module,
          function: t.function,
          typeArguments: t.typeArguments as TypeTag[],
          arguments: t.arguments as CallArg[],
        },
      };

      return await this.constructTransactionData(
        tx,
        t.gasPayment!,
        t.gasBudget,
        signerAddress
      );
    } catch (err) {
      throw new Error(`Error executing a move call: ${err} with args ${t}`);
    }
  }

  async newMergeCoin(
    _signerAddress: SuiAddress,
    _t: MergeCoinTransaction
  ): Promise<Base64DataBuffer> {
    throw new Error('Not implemented');
  }

  async newSplitCoin(
    _signerAddress: SuiAddress,
    _t: SplitCoinTransaction
  ): Promise<Base64DataBuffer> {
    throw new Error('Not implemented');
  }

  async newPublish(
    _signerAddress: SuiAddress,
    _t: PublishTransaction
  ): Promise<Base64DataBuffer> {
    throw new Error('Not implemented');
  }

  private async constructTransactionData(
    tx: MoveCallTx,
    gasObjectId: string,
    gasBudget: number,
    signerAddress: SuiAddress
  ): Promise<Base64DataBuffer> {
    // TODO: mark gasPayment as required in `MoveCallTransaction`
    const gasPayment = await this.provider.getObjectRef(gasObjectId);
    const txData = {
      kind: {
        // TODO: support batch txns
        Single: tx,
      },
      gasPayment: gasPayment!,
      // Need to keep in sync with
      // https://github.com/MystenLabs/sui/blob/f32877f2e40d35a008710c232e49b57aab886462/crates/sui-types/src/messages.rs#L338
      gasPrice: 1,
      gasBudget: gasBudget,
      sender: signerAddress,
    };

    return this.serializeTransactionData(txData);
  }

  private serializeTransactionData(
    tx: TransactionData,
    // TODO: derive the buffer size automatically
    size: number = 2048
  ): Base64DataBuffer {
    const dataBytes = bcs.ser('TransactionData', tx, size).toBytes();
    const typeTag = Array.from('TransactionData::').map(e => e.charCodeAt(0));
    const serialized = new Uint8Array(typeTag.length + dataBytes.length);
    serialized.set(typeTag);
    serialized.set(dataBytes, typeTag.length);
    return new Base64DataBuffer(serialized);
  }
}
