// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';
import {
  bcs,
  Coin,
  PAY_JOIN_COIN_FUNC_NAME,
  PAY_MODULE_NAME,
  SUI_PACKAGE_ID,
  PAY_SPLIT_COIN_VEC_FUNC_NAME,
  ObjectId,
  SuiAddress,
  SUI_TYPE_ARG,
  Transaction,
  TransactionData,
  TypeTag,
  RpcApiVersion,
  SuiObjectRef,
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
  SignableTransaction,
  UnserializedSignableTransaction,
} from './txn-data-serializer';
import { Provider } from '../../providers/provider';
import { CallArgSerializer } from './call-arg-serializer';
import { TypeTagSerializer } from './type-tag-serializer';

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
        { kind: 'transferObject', data: t },
        t.gasPayment,
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
        { kind: 'transferSui', data: t },
        t.suiObjectId,
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
        { kind: 'pay', data: t },
        t.gasPayment,
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
          typeArguments: t.typeArguments.map((a) =>
            typeof a === 'string'
              ? new TypeTagSerializer().parseFromStr(a)
              : (a as TypeTag)
          ),
          arguments: await new CallArgSerializer(
            this.provider
          ).serializeMoveCallArguments(t),
        },
      };

      return await this.constructTransactionData(
        tx,
        { kind: 'moveCall', data: t },
        t.gasPayment,
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
        packageObjectId: SUI_PACKAGE_ID,
        module: PAY_MODULE_NAME,
        function: PAY_JOIN_COIN_FUNC_NAME,
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
        packageObjectId: SUI_PACKAGE_ID,
        module: PAY_MODULE_NAME,
        function: PAY_SPLIT_COIN_VEC_FUNC_NAME,
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
        { kind: 'publish', data: t },
        t.gasPayment,
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

  /**
   * Util function to select a coin for gas payment given an transaction, which will select
   * an arbitrary gas object owned by the address with balance greater than or equal to
   * `txn.data.gasBudget` that's not used in `txn` itself and the `exclude` list.
   *
   * @param txn the transaction for which the gas object is selected
   * @param signerAddress signer of the transaction
   * @param exclude additional object ids of the gas coins to exclude. Object ids that appear
   * in `txn` will be appended
   */
  public async selectGasPaymentForTransaction(
    txn: SignableTransaction,
    signerAddress: string,
    exclude: ObjectId[] = []
  ): Promise<ObjectId | undefined> {
    if (txn.kind === 'bytes') {
      return undefined;
    }

    const coins = await this.provider.selectCoinsWithBalanceGreaterThanOrEqual(
      signerAddress,
      BigInt(txn.data.gasBudget),
      SUI_TYPE_ARG,
      exclude.concat(await this.extractObjectIds(txn))
    );

    return coins.length > 0 ? Coin.getID(coins[0]) : undefined;
  }

  /**
   * Returns a list of object ids used in the transaction, including the gas payment object
   */
  public async extractObjectIds(txn: SignableTransaction): Promise<ObjectId[]> {
    const ret = await this.extractInputObjectIds(txn);
    if ('gasPayment' in txn.data && txn.data['gasPayment']) {
      ret.push(txn.data['gasPayment']);
    }
    return ret;
  }

  private async extractInputObjectIds(
    txn: SignableTransaction
  ): Promise<ObjectId[]> {
    switch (txn.kind) {
      case 'moveCall':
        return await new CallArgSerializer(this.provider).extractObjectIds(
          txn.data
        );
      case 'transferSui':
        return [txn.data.suiObjectId];
      case 'transferObject':
        return [txn.data.objectId];
      case 'mergeCoin':
        return [txn.data.primaryCoin, txn.data.coinToMerge];
      case 'splitCoin':
        return [txn.data.coinObjectId];
      case 'pay':
        return txn.data.inputCoins;
    }
    return [];
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
    originalTx: UnserializedSignableTransaction,
    gasObjectId: ObjectId | undefined,
    signerAddress: SuiAddress
  ): Promise<Base64DataBuffer> {
    if (gasObjectId === undefined) {
      gasObjectId = await this.selectGasPaymentForTransaction(
        originalTx,
        signerAddress
      );
      if (gasObjectId === undefined) {
        throw new Error(
          `Unable to select a gas object with balance greater than or equal to ${originalTx.data.gasBudget}`
        );
      }
    }
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
      gasBudget: originalTx.data.gasBudget,
      sender: signerAddress,
    };

    return await this.serializeTransactionData(txData);
  }

  /**
   * Serialize `TransactionData` into BCS encoded bytes
   */
  public async serializeTransactionData(
    tx: TransactionData,
    // TODO: derive the buffer size automatically
    size: number = 8192
  ): Promise<Base64DataBuffer> {
    const version = await this.provider.getRpcApiVersion();
    const format = shouldUseOldSharedObjectAPI(version)
      ? 'TransactionData_Deprecated'
      : 'TransactionData';

    const dataBytes = bcs.ser(format, tx, size).toBytes();
    const serialized = new Uint8Array(TYPE_TAG.length + dataBytes.length);
    serialized.set(TYPE_TAG);
    serialized.set(dataBytes, TYPE_TAG.length);
    return new Base64DataBuffer(serialized);
  }

  /**
   * Deserialize BCS encoded bytes into `SignableTransaction`
   */
  public async deserializeTransactionBytesToSignableTransaction(
    bytes: Base64DataBuffer
  ): Promise<
    UnserializedSignableTransaction | UnserializedSignableTransaction[]
  > {
    return this.transformTransactionDataToSignableTransaction(
      await this.deserializeTransactionBytesToTransactionData(bytes)
    );
  }

  /**
   * Deserialize BCS encoded bytes into `TransactionData`
   */
  public async deserializeTransactionBytesToTransactionData(
    bytes: Base64DataBuffer
  ): Promise<TransactionData> {
    const version = await this.provider.getRpcApiVersion();
    const format = shouldUseOldSharedObjectAPI(version)
      ? 'TransactionData_Deprecated'
      : 'TransactionData';
    return bcs.de(format, bytes.getData().slice(TYPE_TAG.length));
  }

  /**
   * Deserialize `TransactionData` to `SignableTransaction`
   */
  public async transformTransactionDataToSignableTransaction(
    tx: TransactionData
  ): Promise<
    UnserializedSignableTransaction | UnserializedSignableTransaction[]
  > {
    if ('Single' in tx.kind) {
      return this.transformTransactionToSignableTransaction(
        tx.kind.Single,
        tx.gasBudget,
        tx.gasPayment
      );
    }
    return Promise.all(
      tx.kind.Batch.map((t) =>
        this.transformTransactionToSignableTransaction(
          t,
          tx.gasBudget,
          tx.gasPayment
        )
      )
    );
  }

  public async transformTransactionToSignableTransaction(
    tx: Transaction,
    gasBudget: number,
    gasPayment?: SuiObjectRef
  ): Promise<UnserializedSignableTransaction> {
    if ('Pay' in tx) {
      return {
        kind: 'pay',
        data: {
          inputCoins: tx.Pay.coins.map((c) => c.objectId),
          recipients: tx.Pay.recipients,
          amounts: tx.Pay.amounts,
          gasPayment: gasPayment?.objectId,
          gasBudget,
        },
      };
    } else if ('Call' in tx) {
      return {
        kind: 'moveCall',
        data: {
          packageObjectId: tx.Call.package.objectId,
          module: tx.Call.module,
          function: tx.Call.function,
          typeArguments: tx.Call.typeArguments,
          arguments: await new CallArgSerializer(
            this.provider
          ).deserializeCallArgs(tx),
          gasPayment: gasPayment?.objectId,
          gasBudget,
        },
      };
    } else if ('TransferObject' in tx) {
      return {
        kind: 'transferObject',
        data: {
          objectId: tx.TransferObject.object_ref.objectId,
          recipient: tx.TransferObject.recipient,
          gasPayment: gasPayment?.objectId,
          gasBudget,
        },
      };
    } else if ('TransferSui' in tx) {
      return {
        kind: 'transferSui',
        data: {
          suiObjectId: gasPayment!.objectId,
          recipient: tx.TransferSui.recipient,
          amount:
            'Some' in tx.TransferSui.amount ? tx.TransferSui.amount.Some : null,
          gasBudget,
        },
      };
    } else if ('Publish' in tx) {
      return {
        kind: 'publish',
        data: {
          compiledModules: tx.Publish.modules,
          gasPayment: gasPayment?.objectId,
          gasBudget,
        },
      };
    }
    throw new Error(`Unsupported transaction type ${tx}`);
  }
}

export function shouldUseOldSharedObjectAPI(version?: RpcApiVersion): boolean {
  return version?.major === 0 && version?.minor <= 12;
}
