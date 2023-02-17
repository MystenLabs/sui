// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { SerializedSignature } from '../cryptography/signature';
import { JsonRpcProvider } from '../providers/json-rpc-provider';
import { Provider } from '../providers/provider';
import { VoidProvider } from '../providers/void-provider';
import { HttpHeaders } from '../rpc/client';
import {
  deserializeTransactionBytesToTransactionData,
  ExecuteTransactionRequestType,
  FaucetResponse,
  generateTransactionDigest,
  getTotalGasUsedUpperBound,
  SuiAddress,
  SuiExecuteTransactionResponse,
  TransactionEffects,
  DevInspectResults,
  bcsForVersion,
} from '../types';
import { IntentScope, messageWithIntent } from '../utils/intent';
import { Signer } from './signer';
import { RpcTxnDataSerializer } from './txn-data-serializers/rpc-txn-data-serializer';
import {
  MoveCallTransaction,
  MergeCoinTransaction,
  PayTransaction,
  PaySuiTransaction,
  PayAllSuiTransaction,
  SplitCoinTransaction,
  TransferObjectTransaction,
  TransferSuiTransaction,
  TxnDataSerializer,
  PublishTransaction,
  SignableTransaction,
  UnserializedSignableTransaction,
  SignedTransaction,
} from './txn-data-serializers/txn-data-serializer';

///////////////////////////////
// Exported Abstracts
export abstract class SignerWithProvider implements Signer {
  readonly provider: Provider;
  readonly serializer: TxnDataSerializer;

  ///////////////////
  // Sub-classes MUST implement these

  // Returns the checksum address
  abstract getAddress(): Promise<SuiAddress>;

  /**
   * Returns the signature for the data and the public key of the signer
   */
  abstract signData(data: Uint8Array): Promise<SerializedSignature>;

  // Returns a new instance of the Signer, connected to provider.
  // This MAY throw if changing providers is not supported.
  abstract connect(provider: Provider): SignerWithProvider;

  ///////////////////
  // Sub-classes MAY override these

  /**
   * Request gas tokens from a faucet server and send to the signer
   * address
   * @param httpHeaders optional request headers
   */
  async requestSuiFromFaucet(
    httpHeaders?: HttpHeaders,
  ): Promise<FaucetResponse> {
    return this.provider.requestSuiFromFaucet(
      await this.getAddress(),
      httpHeaders,
    );
  }

  constructor(provider?: Provider, serializer?: TxnDataSerializer) {
    this.provider = provider || new VoidProvider();
    let endpoint = '';
    let skipDataValidation = false;
    if (this.provider instanceof JsonRpcProvider) {
      endpoint = this.provider.endpoints.fullNode;
      skipDataValidation = this.provider.options.skipDataValidation!;
    }
    this.serializer =
      serializer || new RpcTxnDataSerializer(endpoint, skipDataValidation);
  }

  /**
   * Sign a message using the keypair, with the `PersonalMessage` intent.
   */
  async signMessage(message: Uint8Array): Promise<SerializedSignature> {
    return await this.signData(
      messageWithIntent(IntentScope.PersonalMessage, message),
    );
  }

  /**
   * Sign a transaction.
   */
  async signTransaction(
    transaction: Uint8Array | SignableTransaction,
  ): Promise<SignedTransaction> {
    let transactionBytes;
    if (transaction instanceof Uint8Array || transaction.kind === 'bytes') {
      transactionBytes =
        transaction instanceof Uint8Array ? transaction : transaction.data;
    } else {
      transactionBytes = await this.serializer.serializeToBytes(
        await this.getAddress(),
        transaction,
        'Commit',
      );
    }

    const intentMessage = messageWithIntent(
      IntentScope.TransactionData,
      transactionBytes,
    );
    const signature = await this.signData(intentMessage);

    return {
      transactionBytes: toB64(transactionBytes),
      signature,
    };
  }

  /**
   * Sign a transaction and submit to the Fullnode for execution.
   */
  async signAndExecuteTransaction(
    transaction: Uint8Array | SignableTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    const { transactionBytes, signature } = await this.signTransaction(
      transaction,
    );

    return await this.provider.executeTransaction(
      transactionBytes,
      signature,
      requestType,
    );
  }

  async getTransactionDigest(
    tx: Uint8Array | SignableTransaction,
  ): Promise<string> {
    let txBytes: Uint8Array;
    if (tx instanceof Uint8Array || tx.kind === 'bytes') {
      txBytes = tx instanceof Uint8Array ? tx : tx.data;
    } else {
      txBytes = await this.serializer.serializeToBytes(
        await this.getAddress(),
        tx,
        'DevInspect',
      );
    }
    const version = await this.provider.getRpcApiVersion();
    const bcs = bcsForVersion(version);
    const data = deserializeTransactionBytesToTransactionData(bcs, txBytes);
    return generateTransactionDigest(data, bcs);
  }

  /**
   * Runs the transaction in dev-inpsect mode. Which allows for nearly any
   * transaction (or Move call) with any arguments. Detailed results are
   * provided, including both the transaction effects and any return values.
   *
   * @param tx the transaction as SignableTransaction or string (in base64) that will dry run
   * @param gas_price optional. Default to use the network reference gas price stored
   * in the Sui System State object
   * @param epoch optional. Default to use the current epoch number stored
   * in the Sui System State object
   */
  async devInspectTransaction(
    tx: UnserializedSignableTransaction | string | Uint8Array,
    gasPrice: number | null = null,
    epoch: number | null = null,
  ): Promise<DevInspectResults> {
    const address = await this.getAddress();
    return this.provider.devInspectTransaction(address, tx, gasPrice, epoch);
  }

  /**
   * Dry run a transaction and return the result.
   * @param tx the transaction as SignableTransaction or string (in base64) that will dry run
   * @returns The transaction effects
   */
  async dryRunTransaction(
    tx: SignableTransaction | string | Uint8Array,
  ): Promise<TransactionEffects> {
    const address = await this.getAddress();
    let dryRunTxBytes: Uint8Array;
    if (typeof tx === 'string') {
      dryRunTxBytes = fromB64(tx);
    } else if (tx instanceof Uint8Array) {
      dryRunTxBytes = tx;
    } else {
      switch (tx.kind) {
        case 'bytes':
          dryRunTxBytes = tx.data;
          break;
        default:
          dryRunTxBytes = await this.serializer.serializeToBytes(
            address,
            tx,
            'Commit',
          );
          break;
      }
    }
    return this.provider.dryRunTransaction(dryRunTxBytes);
  }

  /**
   *
   * Serialize and sign a `TransferObject` transaction and submit to the Fullnode
   * for execution
   */
  async transferObject(
    transaction: TransferObjectTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'transferObject', data: transaction },
      requestType,
    );
  }

  /**
   *
   * Serialize and sign a `TransferSui` transaction and submit to the Fullnode
   * for execution
   */
  async transferSui(
    transaction: TransferSuiTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'transferSui', data: transaction },
      requestType,
    );
  }

  /**
   *
   * Serialize and Sign a `Pay` transaction and submit to the fullnode for execution
   */
  async pay(
    transaction: PayTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'pay', data: transaction },
      requestType,
    );
  }

  /**
   * Serialize and Sign a `PaySui` transaction and submit to the fullnode for execution
   */
  async paySui(
    transaction: PaySuiTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'paySui', data: transaction },
      requestType,
    );
  }

  /**
   * Serialize and Sign a `PayAllSui` transaction and submit to the fullnode for execution
   */
  async payAllSui(
    transaction: PayAllSuiTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'payAllSui', data: transaction },
      requestType,
    );
  }

  /**
   *
   * Serialize and sign a `MergeCoin` transaction and submit to the Fullnode
   * for execution
   */
  async mergeCoin(
    transaction: MergeCoinTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'mergeCoin', data: transaction },
      requestType,
    );
  }

  /**
   *
   * Serialize and sign a `SplitCoin` transaction and submit to the Fullnode
   * for execution
   */
  async splitCoin(
    transaction: SplitCoinTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'splitCoin', data: transaction },
      requestType,
    );
  }

  /**
   * Serialize and sign a `MoveCall` transaction and submit to the Fullnode
   * for execution
   */
  async executeMoveCall(
    transaction: MoveCallTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'moveCall', data: transaction },
      requestType,
    );
  }

  /**
   *
   * Serialize and sign a `Publish` transaction and submit to the Fullnode
   * for execution
   */
  async publish(
    transaction: PublishTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    return this.signAndExecuteTransaction(
      { kind: 'publish', data: transaction },
      requestType,
    );
  }

  /**
   * Returns the estimated gas cost for the transaction
   * @param tx The transaction to estimate the gas cost. When string it is assumed it's a serialized tx in base64
   * @returns total gas cost estimation
   * @throws whens fails to estimate the gas cost
   */
  async getGasCostEstimation(
    ...args: Parameters<SignerWithProvider['dryRunTransaction']>
  ) {
    const txEffects = await this.dryRunTransaction(...args);
    const gasEstimation = getTotalGasUsedUpperBound(txEffects);
    if (typeof gasEstimation === 'undefined') {
      throw new Error('Failed to estimate the gas cost from transaction');
    }
    return gasEstimation;
  }
}
