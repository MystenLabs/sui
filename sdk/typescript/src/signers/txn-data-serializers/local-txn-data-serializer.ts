// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';
import {
  bcs,
  Coin,
  COIN_JOIN_FUNC_NAME,
  COIN_MODULE_NAME,
  COIN_PACKAGE_ID,
  COIN_SPLIT_VEC_FUNC_NAME,
  SuiAddress,
  Transaction,
  TransactionData,
  TypeTag,
} from '../../types';
import {
  MoveCallTransaction,
  MergeCoinTransaction,
  SplitCoinTransaction,
  TransferObjectTransaction,
  TransferSuiTransaction,
  PublishTransaction,
  TxnDataSerializer,
  PayTransaction,
} from './txn-data-serializer';
import { Provider } from '../../providers/provider';
import { CallArgSerializer } from './call-arg-serializer';

const TYPE_TAG = Array.from('TransactionData::').map((e) => e.charCodeAt(0));

export class LocalTxnDataSerializer implements TxnDataSerializer {
  /**
   * Need a provider to fetch the latest object reference. Ideally the provider
   * should cache the object reference locally
   */
  constructor(private provider: Provider) {}

  async newTransferObject(
    signerAddress: SuiAddress,
    t: TransferObjectTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const objectRef = await this.provider.getObjectRef(t.objectId);
      const tx = {
        TransferObject: {
          recipient: t.recipient,
          object_ref: objectRef!,
        },
      };
      return await this.constructTransactionData(
        tx,
        // TODO: make `gasPayment` a required field in `TransferObjectTransaction`
        t.gasPayment!,
        t.gasBudget,
        signerAddress
      );
    } catch (err) {
      throw new Error(
        `Error constructing a TransferObject transaction: ${err} args ${JSON.stringify(
          t
        )}`
      );
    }
  }

  async newTransferSui(
    signerAddress: SuiAddress,
    t: TransferSuiTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const tx = {
        TransferSui: {
          recipient: t.recipient,
          amount: t.amount == null ? { None: null } : { Some: t.amount },
        },
      };
      return await this.constructTransactionData(
        tx,
        t.suiObjectId,
        t.gasBudget,
        signerAddress
      );
    } catch (err) {
      throw new Error(
        `Error constructing a TransferSui transaction: ${err} args ${JSON.stringify(
          t
        )}`
      );
    }
  }

  async newPay(
    signerAddress: SuiAddress,
    t: PayTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const inputCoinRefs = (
        await Promise.all(
          t.inputCoins.map((coin) => this.provider.getObjectRef(coin))
        )
      ).map((ref) => ref!);
      const tx = {
        Pay: {
          coins: inputCoinRefs,
          recipients: t.recipients,
          amounts: t.amounts,
        },
      };
      return await this.constructTransactionData(
        tx,
        t.gasPayment!,
        t.gasBudget,
        signerAddress
      );
    } catch (err) {
      throw new Error(
        `Error constructing a Pay transaction: ${err} args ${JSON.stringify(t)}`
      );
    }
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
          arguments: await new CallArgSerializer(
            this.provider
          ).serializeMoveCallArguments(t),
        },
      };

      return await this.constructTransactionData(
        tx,
        // TODO: make `gasPayment` a required field in `MoveCallTransaction`
        t.gasPayment!,
        t.gasBudget,
        signerAddress
      );
    } catch (err) {
      throw new Error(
        `Error constructing a move call: ${err} args ${JSON.stringify(t)}`
      );
    }
  }

  async newMergeCoin(
    signerAddress: SuiAddress,
    t: MergeCoinTransaction
  ): Promise<Base64DataBuffer> {
    try {
      return await this.newMoveCall(signerAddress, {
        packageObjectId: COIN_PACKAGE_ID,
        module: COIN_MODULE_NAME,
        function: COIN_JOIN_FUNC_NAME,
        typeArguments: [await this.getCoinStructTag(t.coinToMerge)],
        arguments: [t.primaryCoin, t.coinToMerge],
        gasPayment: t.gasPayment,
        gasBudget: t.gasBudget,
      });
    } catch (err) {
      throw new Error(
        `Error constructing a MergeCoin Transaction: ${err} args ${JSON.stringify(
          t
        )}`
      );
    }
  }

  async newSplitCoin(
    signerAddress: SuiAddress,
    t: SplitCoinTransaction
  ): Promise<Base64DataBuffer> {
    try {
      return await this.newMoveCall(signerAddress, {
        packageObjectId: COIN_PACKAGE_ID,
        module: COIN_MODULE_NAME,
        function: COIN_SPLIT_VEC_FUNC_NAME,
        typeArguments: [await this.getCoinStructTag(t.coinObjectId)],
        arguments: [t.coinObjectId, t.splitAmounts],
        gasPayment: t.gasPayment,
        gasBudget: t.gasBudget,
      });
    } catch (err) {
      throw new Error(
        `Error constructing a SplitCoin Transaction: ${err} args ${JSON.stringify(
          t
        )}`
      );
    }
  }

  async newPublish(
    signerAddress: SuiAddress,
    t: PublishTransaction
  ): Promise<Base64DataBuffer> {
    try {
      const tx = {
        Publish: {
          modules: t.compiledModules as ArrayLike<ArrayLike<number>>,
        },
      };
      return await this.constructTransactionData(
        tx,
        // TODO: make `gasPayment` a required field in `PublishTransaction`
        t.gasPayment!,
        t.gasBudget,
        signerAddress
      );
    } catch (err) {
      throw new Error(
        `Error constructing a newPublish transaction: ${err} with args ${JSON.stringify(
          t
        )}`
      );
    }
  }

  private async getCoinStructTag(coinId: string): Promise<TypeTag> {
    const coin = await this.provider.getObject(coinId);
    const coinTypeArg = Coin.getCoinTypeArg(coin);
    if (coinTypeArg == null) {
      throw new Error(`Object ${coinId} is not a valid coin type`);
    }
    return { struct: Coin.getCoinStructTag(coinTypeArg) };
  }

  private async constructTransactionData(
    tx: Transaction,
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

    console.log('transactiondata', txData);

    return this.serializeTransactionData(txData);
  }

  private serializeTransactionData(
    tx: TransactionData,
    // TODO: derive the buffer size automatically
    size: number = 2048
  ): Base64DataBuffer {
    const dataBytes = bcs.ser('TransactionData', tx, size).toBytes();
    const serialized = new Uint8Array(TYPE_TAG.length + dataBytes.length);
    serialized.set(TYPE_TAG);
    serialized.set(dataBytes, TYPE_TAG.length);
    return new Base64DataBuffer(serialized);
  }
}
