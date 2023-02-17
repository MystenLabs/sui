// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
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
  TransactionKind,
  TypeTag,
  SuiObjectRef,
  deserializeTransactionBytesToTransactionData,
  normalizeSuiObjectId,
  bcsForVersion,
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
    _mode: TransactionBuilderMode = 'Commit',
  ): Promise<Uint8Array> {
    try {
      return await this.serializeTransactionData(
        await this.constructTransactionData(signerAddress, txn),
      );
    } catch (e) {
      throw new Error(
        `Encountered error when serializing a ${txn.kind} transaction for ` +
          `address ${signerAddress} for transaction ${JSON.stringify(
            txn,
            null,
            2,
          )}: ${e}`,
      );
    }
  }

  /**
   * Serialize a signable transaction without gasPayment, gasPrice, and gasBudget
   * This is useful for the DevInspect endpoint for simulating the transaction
   */
  async serializeToBytesWithoutGasInfo(
    signerAddress: string,
    txn: UnserializedSignableTransaction,
  ): Promise<Uint8Array> {
    try {
      return await this.serializeTransactionKind(
        (
          await this.constructTransactionKindAndPayment(signerAddress, txn)
        )[0],
      );
    } catch (e) {
      throw new Error(
        `Encountered error when serializing a ${txn.kind} transaction without gas info for ` +
          `address ${signerAddress} for transaction ${JSON.stringify(
            txn,
            null,
            2,
          )}: ${e}`,
      );
    }
  }

  async constructTransactionKindAndPayment(
    signerAddress: string,
    unserializedTxn: UnserializedSignableTransaction,
  ): Promise<[TransactionKind, ObjectId | undefined]> {
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
            pay.inputCoins.map((coin) => this.provider.getObjectRef(coin)),
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
            paySui.inputCoins.map((coin) => this.provider.getObjectRef(coin)),
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
            payAllSui.inputCoins.map((coin) =>
              this.provider.getObjectRef(coin),
            ),
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
        const api = await this.provider.getRpcApiVersion();

        // TODO: remove after 0.24.0 is deployed for devnet and testnet
        const pkg =
          api?.major === 0 && api?.minor < 24
            ? (await this.provider.getObjectRef(moveCall.packageObjectId))!
            : normalizeSuiObjectId(moveCall.packageObjectId);

        tx = {
          Call: {
            package: pkg,
            module: moveCall.module,
            function: moveCall.function,
            typeArguments: moveCall.typeArguments.map((a) =>
              typeof a === 'string'
                ? TypeTagSerializer.parseFromStr(a, true)
                : (a as TypeTag),
            ),
            arguments: await new CallArgSerializer(
              this.provider,
            ).serializeMoveCallArguments(moveCall),
          },
        };
        gasPayment = moveCall.gasPayment;
        break;
      case 'mergeCoin':
        const mergeCoin = unserializedTxn.data as MergeCoinTransaction;
        return this.constructTransactionKindAndPayment(signerAddress, {
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
        return this.constructTransactionKindAndPayment(signerAddress, {
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
    // TODO: support batch transaction
    return [{ Single: tx }, gasPayment];
  }

  async constructTransactionData(
    signerAddress: string,
    unserializedTxn: UnserializedSignableTransaction,
  ): Promise<TransactionData> {
    const [tx, gasPayment] = await this.constructTransactionKindAndPayment(
      signerAddress,
      unserializedTxn,
    );
    return this.constructTransactionDataHelper(
      tx,
      unserializedTxn,
      gasPayment,
      signerAddress,
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
    exclude: ObjectId[] = [],
  ): Promise<ObjectId | undefined> {
    if (txn.kind === 'bytes') {
      return undefined;
    }
    const requiredGasAmount =
      BigInt(txn.data.gasBudget!) * BigInt(txn.data.gasPrice!);
    const coins = await this.provider.selectCoinsWithBalanceGreaterThanOrEqual(
      signerAddress,
      requiredGasAmount,
      SUI_TYPE_ARG,
      exclude.concat(await this.extractObjectIds(txn)),
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
    txn: SignableTransaction,
  ): Promise<ObjectId[]> {
    switch (txn.kind) {
      case 'moveCall':
        return await new CallArgSerializer(this.provider).extractObjectIds(
          txn.data,
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
    tx: TransactionKind,
    originalTx: UnserializedSignableTransaction,
    gasObjectId: ObjectId | undefined,
    signerAddress: SuiAddress,
  ): Promise<TransactionData> {
    // TODO: Allow people to add tip to the reference gas price by using originalTx.data.gasPrice
    originalTx.data.gasPrice = await this.provider.getReferenceGasPrice();
    if (gasObjectId === undefined) {
      gasObjectId = await this.selectGasPaymentForTransaction(
        originalTx,
        signerAddress,
      );
      if (gasObjectId === undefined) {
        throw new Error(
          `Unable to select a gas object with balance greater than or equal to ${originalTx.data.gasBudget}`,
        );
      }
    }
    const gasPayment = await this.provider.getObjectRef(gasObjectId);
    if (!originalTx.data.gasBudget) {
      throw new Error(
        'Must provide a valid gas budget for contructing TransactionData',
      );
    }
    return {
      kind: tx,
      gasPayment: gasPayment!,
      gasPrice: originalTx.data.gasPrice!,
      gasBudget: originalTx.data.gasBudget!,
      sender: signerAddress,
    };
  }

  /**
   * Serialize `TransactionData` into BCS encoded bytes
   */
  public async serializeTransactionData(
    tx: TransactionData,
    // TODO: derive the buffer size automatically
    size: number = 8192,
  ): Promise<Uint8Array> {
    const bcs = bcsForVersion(await this.provider.getRpcApiVersion());
    const dataBytes = bcs.ser('TransactionData', tx, size).toBytes();
    return dataBytes;
  }

  /**
   * Serialize `TransactionKind` into BCS encoded bytes
   */
  public async serializeTransactionKind(
    tx: TransactionKind,
    // TODO: derive the buffer size automatically
    size: number = 8192,
  ): Promise<Uint8Array> {
    const bcs = bcsForVersion(await this.provider.getRpcApiVersion());
    const dataBytes = bcs.ser('TransactionKind', tx, size).toBytes();
    return dataBytes;
  }

  /**
   * Deserialize BCS encoded bytes into `SignableTransaction`
   */
  public async deserializeTransactionBytesToSignableTransaction(
    bytes: Uint8Array,
  ): Promise<
    UnserializedSignableTransaction | UnserializedSignableTransaction[]
  > {
    let version = await this.provider.getRpcApiVersion();
    return this.transformTransactionDataToSignableTransaction(
      deserializeTransactionBytesToTransactionData(
        bcsForVersion(version),
        bytes,
      ),
    );
  }

  /**
   * Deserialize `TransactionData` to `SignableTransaction`
   */
  public async transformTransactionDataToSignableTransaction(
    tx: TransactionData,
  ): Promise<
    UnserializedSignableTransaction | UnserializedSignableTransaction[]
  > {
    if ('Single' in tx.kind) {
      return this.transformTransactionToSignableTransaction(
        tx.kind.Single,
        tx.gasBudget,
        tx.gasPayment,
        tx.gasPrice,
      );
    }
    return Promise.all(
      tx.kind.Batch.map((t) =>
        this.transformTransactionToSignableTransaction(
          t,
          tx.gasBudget,
          tx.gasPayment,
          tx.gasPrice,
        ),
      ),
    );
  }

  public async transformTransactionToSignableTransaction(
    tx: Transaction,
    gasBudget: number,
    gasPayment?: SuiObjectRef,
    gasPrice?: number,
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
          gasPrice,
        },
      };
    } else if ('Call' in tx) {
      const packageObjectId =
        typeof tx.Call.package === 'string'
          ? tx.Call.package
          : tx.Call.package.objectId;

      return {
        kind: 'moveCall',
        data: {
          packageObjectId,
          module: tx.Call.module,
          function: tx.Call.function,
          typeArguments: tx.Call.typeArguments,
          arguments: await new CallArgSerializer(
            this.provider,
          ).deserializeCallArgs(tx),
          gasPayment: gasPayment?.objectId,
          gasBudget,
          gasPrice,
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
          gasPrice,
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
          gasPrice,
        },
      };
    } else if ('Publish' in tx) {
      return {
        kind: 'publish',
        data: {
          compiledModules: tx.Publish.modules,
          gasPayment: gasPayment?.objectId,
          gasBudget,
          gasPrice,
        },
      };
    } else if ('PaySui' in tx) {
      return {
        kind: 'paySui',
        data: {
          inputCoins: tx.PaySui.coins.map((c) => c.objectId),
          recipients: tx.PaySui.recipients,
          amounts: tx.PaySui.amounts,
        },
      };
    } else if ('PayAllSui' in tx) {
      return {
        kind: 'payAllSui',
        data: {
          inputCoins: tx.PayAllSui.coins.map((c) => c.objectId),
          recipient: tx.PayAllSui.recipient,
        },
      };
    }
    throw new Error(`Unsupported transaction type ${tx}`);
  }
}
