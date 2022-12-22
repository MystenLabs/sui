// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';
import {
  bcs,
  Coin,
  PAY_JOIN_COIN_FUNC_NAME,
  PAY_MODULE_NAME,
  SUI_FRAMEWORK_ADDRESS,
  PAY_SPLIT_COIN_VEC_FUNC_NAME,
  ObjectId,
  SuiAddress,
  SUI_TYPE_ARG,
  Transaction,
  TransactionData,
  TypeTag,
  SuiObjectRef,
  TRANSACTION_DATA_TYPE_TAG,
  deserializeTransactionBytesToTransactionData,
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
  PaySuiTransaction,
  PayAllSuiTransaction,
  SignableTransaction,
  UnserializedSignableTransaction,
  TransactionBuilderMode,
} from './txn-data-serializer';
import { Provider } from '../../providers/provider';
import { CallArgSerializer } from './call-arg-serializer';
import { TypeTagSerializer } from './type-tag-serializer';

export class LocalTxnDataSerializer implements TxnDataSerializer {
  /**
   * Need a provider to fetch the latest object reference. Ideally the provider
   * should cache the object reference locally
   */
  constructor(private provider: Provider) {}

  async serializeToBytes(
    signerAddress: string,
    txn: UnserializedSignableTransaction,
    _mode: TransactionBuilderMode = 'Commit'
  ): Promise<Base64DataBuffer> {
    try {
      const version = await this.provider.getRpcApiVersion();
      const useIntentSigning =
        version != null && version.major >= 0 && version.minor > 18;
      return await this.serializeTransactionData(
        useIntentSigning,
        await this.constructTransactionData(signerAddress, txn)
      );
    } catch (e) {
      throw new Error(
        `Encountered error when serializing a ${txn.kind} transaction for ` +
          `address ${signerAddress} for transaction ${JSON.stringify(
            txn,
            null,
            2
          )}: ${e}`
      );
    }
  }

  async constructTransactionData(
    signerAddress: string,
    unserializedTxn: UnserializedSignableTransaction
  ): Promise<TransactionData> {
    let tx: Transaction;
    let gasPayment: ObjectId | undefined;
    switch (unserializedTxn.kind) {
      case 'transferObject':
        const t = unserializedTxn.data as TransferObjectTransaction;
        const objectRef = await this.provider.getObjectRef(t.objectId);
        tx = {
          TransferObject: {
            recipient: t.recipient,
            object_ref: objectRef!,
          },
        };
        gasPayment = t.gasPayment;
        break;
      case 'transferSui':
        const transferSui = unserializedTxn.data as TransferSuiTransaction;
        tx = {
          TransferSui: {
            recipient: transferSui.recipient,
            amount:
              transferSui.amount == null
                ? { None: null }
                : { Some: transferSui.amount },
          },
        };
        gasPayment = transferSui.suiObjectId;
        break;
      case 'pay':
        const pay = unserializedTxn.data as PayTransaction;
        const inputCoinRefs = (
          await Promise.all(
            pay.inputCoins.map((coin) => this.provider.getObjectRef(coin))
          )
        ).map((ref) => ref!);
        tx = {
          Pay: {
            coins: inputCoinRefs,
            recipients: pay.recipients,
            amounts: pay.amounts,
          },
        };
        gasPayment = pay.gasPayment;
        break;
      case 'paySui':
        const paySui = unserializedTxn.data as PaySuiTransaction;
        const paySuiInputCoinRefs = (
          await Promise.all(
            paySui.inputCoins.map((coin) => this.provider.getObjectRef(coin))
          )
        ).map((ref) => ref!);
        tx = {
          PaySui: {
            coins: paySuiInputCoinRefs,
            recipients: paySui.recipients,
            amounts: paySui.amounts,
          },
        };
        gasPayment = paySui.inputCoins[0];
        break;
      case 'payAllSui':
        const payAllSui = unserializedTxn.data as PayAllSuiTransaction;
        const payAllSuiInputCoinRefs = (
          await Promise.all(
            payAllSui.inputCoins.map((coin) => this.provider.getObjectRef(coin))
          )
        ).map((ref) => ref!);
        tx = {
          PayAllSui: {
            coins: payAllSuiInputCoinRefs,
            recipient: payAllSui.recipient,
          },
        };
        gasPayment = payAllSui.inputCoins[0];
        break;
      case 'moveCall':
        const moveCall = unserializedTxn.data as MoveCallTransaction;
        const pkg = await this.provider.getObjectRef(moveCall.packageObjectId);
        tx = {
          Call: {
            package: pkg!,
            module: moveCall.module,
            function: moveCall.function,
            typeArguments: moveCall.typeArguments.map((a) =>
              typeof a === 'string'
                ? new TypeTagSerializer().parseFromStr(a)
                : (a as TypeTag)
            ),
            arguments: await new CallArgSerializer(
              this.provider
            ).serializeMoveCallArguments(moveCall),
          },
        };
        gasPayment = moveCall.gasPayment;
        break;
      case 'mergeCoin':
        const mergeCoin = unserializedTxn.data as MergeCoinTransaction;
        return this.constructTransactionData(signerAddress, {
          kind: 'moveCall',
          data: {
            packageObjectId: SUI_FRAMEWORK_ADDRESS,
            module: PAY_MODULE_NAME,
            function: PAY_JOIN_COIN_FUNC_NAME,
            typeArguments: [await this.getCoinStructTag(mergeCoin.coinToMerge)],
            arguments: [mergeCoin.primaryCoin, mergeCoin.coinToMerge],
            gasPayment: mergeCoin.gasPayment,
            gasBudget: mergeCoin.gasBudget,
          },
        });
      case 'splitCoin':
        const splitCoin = unserializedTxn.data as SplitCoinTransaction;
        return this.constructTransactionData(signerAddress, {
          kind: 'moveCall',
          data: {
            packageObjectId: SUI_FRAMEWORK_ADDRESS,
            module: PAY_MODULE_NAME,
            function: PAY_SPLIT_COIN_VEC_FUNC_NAME,
            typeArguments: [
              await this.getCoinStructTag(splitCoin.coinObjectId),
            ],
            arguments: [splitCoin.coinObjectId, splitCoin.splitAmounts],
            gasPayment: splitCoin.gasPayment,
            gasBudget: splitCoin.gasBudget,
          },
        });
      case 'publish':
        const publish = unserializedTxn.data as PublishTransaction;
        tx = {
          Publish: {
            modules: publish.compiledModules as ArrayLike<ArrayLike<number>>,
          },
        };
        gasPayment = publish.gasPayment;
        break;
    }
    return this.constructTransactionDataHelper(
      tx,
      unserializedTxn,
      gasPayment,
      signerAddress
    );
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

  private async constructTransactionDataHelper(
    tx: Transaction,
    originalTx: UnserializedSignableTransaction,
    gasObjectId: ObjectId | undefined,
    signerAddress: SuiAddress
  ): Promise<TransactionData> {
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
    return {
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
  }

  /**
   * Serialize `TransactionData` into BCS encoded bytes
   */
  public async serializeTransactionData(
    useIntentSigning: boolean,
    tx: TransactionData,
    // TODO: derive the buffer size automatically
    size: number = 8192
  ): Promise<Base64DataBuffer> {
    const dataBytes = bcs.ser('TransactionData', tx, size).toBytes();
    if (useIntentSigning) {
      // If use intent signing, do not append type tag. This is mirrored in the rpc tx data serializer TransactionBytes::from_data.
      return new Base64DataBuffer(dataBytes);
    } else {
      const serialized = new Uint8Array(
        TRANSACTION_DATA_TYPE_TAG.length + dataBytes.length
      );
      serialized.set(TRANSACTION_DATA_TYPE_TAG);
      serialized.set(dataBytes, TRANSACTION_DATA_TYPE_TAG.length);
      return new Base64DataBuffer(serialized);
    }
  }

  /**
   * Deserialize BCS encoded bytes into `SignableTransaction`
   */
  public async deserializeTransactionBytesToSignableTransaction(
    useIntentSigning: boolean,
    bytes: Base64DataBuffer
  ): Promise<
    UnserializedSignableTransaction | UnserializedSignableTransaction[]
  > {
    return this.transformTransactionDataToSignableTransaction(
      deserializeTransactionBytesToTransactionData(useIntentSigning, bytes)
    );
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
