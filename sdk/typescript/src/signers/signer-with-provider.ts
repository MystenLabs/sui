// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '../providers/json-rpc-provider';
import { Provider } from '../providers/provider';
import { VoidProvider } from '../providers/void-provider';
import { HttpHeaders } from '../rpc/client';
import { Base64DataBuffer } from '../serialization/base64';
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
import {
  SignaturePubkeyPair,
  SignaturePubkeyPairSerialized,
  Signer,
} from './signer';
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
  SignedTransactionSerialized,
} from './txn-data-serializers/txn-data-serializer';

// See: sui/crates/sui-types/src/intent.rs
enum AppId {
  Sui = 0,
}

enum IntentVersion {
  V0 = 0,
}

enum IntentScope {
  TransactionData = 0,
  TransactionEffects = 1,
  CheckpointSummary = 2,
  PersonalMessage = 3,
}

function intentWithScope(scope: IntentScope) {
  return [scope, IntentVersion.V0, AppId.Sui];
}

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
  abstract signData(
    data: Base64DataBuffer,
    format: 'string',
  ): Promise<SignaturePubkeyPairSerialized>;
  abstract signData(
    data: Base64DataBuffer,
    format?: 'buffer',
  ): Promise<SignaturePubkeyPair>;
  abstract signData(
    data: Base64DataBuffer,
    format?: 'string' | 'buffer',
  ): Promise<SignaturePubkeyPair | SignaturePubkeyPairSerialized>;

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
   * If the `format` argument is set to "buffer", then a Base64DataBuffer will be used to represent the data.
   * If the `format` argument is set to "string", then a JSON-friendly representation will be used instead.
   */
  async signMessage(
    message: Uint8Array | Base64DataBuffer,
    format: 'string',
  ): Promise<SignaturePubkeyPairSerialized>;
  async signMessage(
    message: Uint8Array | Base64DataBuffer,
    format: 'buffer',
  ): Promise<SignaturePubkeyPair>;
  async signMessage(
    message: Uint8Array | Base64DataBuffer,
    format?: 'buffer' | 'string',
  ): Promise<SignaturePubkeyPair | SignaturePubkeyPairSerialized> {
    const signBytes =
      message instanceof Base64DataBuffer
        ? message
        : new Base64DataBuffer(message);

    const intent = intentWithScope(IntentScope.PersonalMessage);
    const intentMessage = new Uint8Array(intent.length + signBytes.getLength());
    intentMessage.set(intent);
    intentMessage.set(signBytes.getData(), intent.length);
    const dataToSign = new Base64DataBuffer(intentMessage);
    return await this.signData(dataToSign, format);
  }

  /**
   * Sign a transaction using the keypair.
   * If the `format` argument is set to "buffer", then a Base64DataBuffer will be used to represent the data.
   * If the `format` argument is set to "string", then a JSON-friendly representation will be used instead.
   */
  async signTransaction(
    transaction: Base64DataBuffer | SignableTransaction,
    format: 'string',
  ): Promise<SignedTransactionSerialized>;
  async signTransaction(
    transaction: Base64DataBuffer | SignableTransaction,
    format?: 'buffer',
  ): Promise<SignedTransaction>;
  async signTransaction(
    transaction: Base64DataBuffer | SignableTransaction,
    format?: 'string' | 'buffer',
  ): Promise<SignedTransaction | SignedTransactionSerialized> {
    let transactionBytes;
    if (
      transaction instanceof Base64DataBuffer ||
      transaction.kind === 'bytes'
    ) {
      transactionBytes =
        transaction instanceof Base64DataBuffer
          ? transaction
          : new Base64DataBuffer(transaction.data);
    } else {
      transactionBytes = await this.serializer.serializeToBytes(
        await this.getAddress(),
        transaction,
        'Commit',
      );
    }

    const intent = intentWithScope(IntentScope.TransactionData);
    const intentMessage = new Uint8Array(
      intent.length + transactionBytes.getLength(),
    );
    intentMessage.set(intent);
    intentMessage.set(transactionBytes.getData(), intent.length);
    const dataToSign = new Base64DataBuffer(intentMessage);
    const signature = await this.signData(dataToSign, format);

    return {
      transactionBytes:
        format === 'string' ? transactionBytes.toString() : transactionBytes,
      signature,
    } as SignedTransaction;
  }
  /**
   * Sign a transaction and submit to the Fullnode for execution. Only exists
   * on Fullnode
   */
  async signAndExecuteTransaction(
    transaction: Base64DataBuffer | SignableTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiExecuteTransactionResponse> {
    const { transactionBytes, signature } = await this.signTransaction(
      transaction,
    );

    return await this.provider.executeTransaction(
      transactionBytes,
      signature.signatureScheme,
      signature.signature,
      signature.pubKey,
      requestType,
    );
  }

  async getTransactionDigest(
    tx: Base64DataBuffer | SignableTransaction,
  ): Promise<string> {
    let txBytes: Base64DataBuffer;
    if (tx instanceof Base64DataBuffer || tx.kind === 'bytes') {
      txBytes =
        tx instanceof Base64DataBuffer ? tx : new Base64DataBuffer(tx.data);
    } else {
      txBytes = await this.serializer.serializeToBytes(
        await this.getAddress(),
        tx,
        'DevInspect',
      );
    }
    const version = await this.provider.getRpcApiVersion();
    const intent = intentWithScope(IntentScope.TransactionData);
    const intentMessage = new Uint8Array(intent.length + txBytes.getLength());
    intentMessage.set(intent);
    intentMessage.set(txBytes.getData(), intent.length);
    const dataToSign = new Base64DataBuffer(intentMessage);

    const bcs = bcsForVersion(version);
    const sig = await this.signData(dataToSign);
    const data = deserializeTransactionBytesToTransactionData(bcs, txBytes);
    return generateTransactionDigest(
      data,
      sig.signatureScheme,
      sig.signature,
      sig.pubKey,
      bcs,
    );
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
    tx: UnserializedSignableTransaction | string | Base64DataBuffer,
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
    tx: SignableTransaction | string | Base64DataBuffer,
  ): Promise<TransactionEffects> {
    const address = await this.getAddress();
    let dryRunTxBytes: string;
    if (typeof tx === 'string') {
      dryRunTxBytes = tx;
    } else if (tx instanceof Base64DataBuffer) {
      dryRunTxBytes = tx.toString();
    } else {
      switch (tx.kind) {
        case 'bytes':
          dryRunTxBytes = new Base64DataBuffer(tx.data).toString();
          break;
        default:
          dryRunTxBytes = (
            await this.serializer.serializeToBytes(address, tx, 'Commit')
          ).toString();
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
