// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '../providers/json-rpc-provider';
import { Provider } from '../providers/provider';
import { VoidProvider } from '../providers/void-provider';
import { HttpHeaders } from '../rpc/client';
import { Base64DataBuffer } from '../serialization/base64';
import {
  ExecuteTransactionRequestType,
  FaucetResponse,
  getTotalGasUsed,
  SuiAddress,
  SuiExecuteTransactionResponse,
  TransactionEffects,
} from '../types';
import { SignaturePubkeyPair, Signer } from './signer';
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
  abstract signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair>;

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
    httpHeaders?: HttpHeaders
  ): Promise<FaucetResponse> {
    return this.provider.requestSuiFromFaucet(
      await this.getAddress(),
      httpHeaders
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
   * Sign a transaction and submit to the Fullnode for execution. Only exists
   * on Fullnode
   */
  async signAndExecuteTransaction(
    transaction: Base64DataBuffer | SignableTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    // Handle submitting raw transaction bytes:
    if (
      transaction instanceof Base64DataBuffer ||
      transaction.kind === 'bytes'
    ) {
      const txBytes =
        transaction instanceof Base64DataBuffer
          ? transaction
          : new Base64DataBuffer(transaction.data);

      const sig = await this.signData(txBytes);
      return await this.provider.executeTransaction(
        txBytes.toString(),
        sig.signatureScheme,
        sig.signature.toString(),
        sig.pubKey.toString(),
        requestType
      );
    }

    switch (transaction.kind) {
      case 'moveCall':
        return this.executeMoveCall(transaction.data, requestType);
      case 'transferSui':
        return this.transferSui(transaction.data, requestType);
      case 'transferObject':
        return this.transferObject(transaction.data, requestType);
      case 'mergeCoin':
        return this.mergeCoin(transaction.data, requestType);
      case 'splitCoin':
        return this.splitCoin(transaction.data, requestType);
      case 'pay':
        return this.pay(transaction.data, requestType);
      case 'paySui':
        return this.paySui(transaction.data, requestType);
      case 'payAllSui':
        return this.payAllSui(transaction.data, requestType);
      case 'publish':
        return this.publish(transaction.data, requestType);
      default:
        throw new Error(
          `Unknown transaction kind: "${(transaction as any).kind}"`
        );
    }
  }

  /**
   * Dry run a transaction and return the result.
   * @param tx the transaction as SignableTransaction or string (in base64) that will dry run
   * @returns The transaction effects
   */
  async dryRunTransaction(tx: SignableTransaction | string | Base64DataBuffer): Promise<TransactionEffects> {
    const address = await this.getAddress();
    let dryRunTxBytes: string;
    if (typeof tx === 'string') {
      dryRunTxBytes = tx;
    } else if (tx instanceof Base64DataBuffer){
      dryRunTxBytes = tx.toString();
    }else{
      switch (tx.kind) {
        case 'bytes':
          dryRunTxBytes = new Base64DataBuffer(tx.data).toString();
          break;
        case 'mergeCoin':
          dryRunTxBytes = (await this.serializer.newMergeCoin(address, tx.data)).toString();
          break;
        case 'moveCall':
          dryRunTxBytes = (await this.serializer.newMoveCall(address, tx.data)).toString();
          break;
        case 'pay':
          dryRunTxBytes = (await this.serializer.newPay(address, tx.data)).toString();
          break;
        case 'payAllSui':
          dryRunTxBytes = (await this.serializer.newPayAllSui(address, tx.data)).toString();
          break;
        case 'paySui':
          dryRunTxBytes = (await this.serializer.newPaySui(address, tx.data)).toString();
          break;
        case 'publish':
          dryRunTxBytes = (await this.serializer.newPublish(address, tx.data)).toString();
          break;
        case 'splitCoin':
          dryRunTxBytes = (await this.serializer.newSplitCoin(address, tx.data)).toString();
          break;
        case 'transferObject':
          dryRunTxBytes = (await this.serializer.newTransferObject(address, tx.data)).toString();
          break;
        case 'transferSui':
          dryRunTxBytes = (await this.serializer.newTransferSui(address, tx.data)).toString();
          break;
        default:
          throw new Error(`Error, unknown transaction kind ${(tx as any).kind}. Can't dry run transaction.`);
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
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newTransferObject(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   *
   * Serialize and sign a `TransferSui` transaction and submit to the Fullnode
   * for execution
   */
  async transferSui(
    transaction: TransferSuiTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newTransferSui(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   *
   * Serialize and Sign a `Pay` transaction and submit to the fullnode for execution
   */
  async pay(
    transaction: PayTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newPay(signerAddress, transaction);
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   * Serialize and Sign a `PaySui` transaction and submit to the fullnode for execution
   */
  async paySui(
    transaction: PaySuiTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newPaySui(signerAddress, transaction);
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   * Serialize and Sign a `PayAllSui` transaction and submit to the fullnode for execution
   */
  async payAllSui(
    transaction: PayAllSuiTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newPayAllSui(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   *
   * Serialize and sign a `MergeCoin` transaction and submit to the Fullnode
   * for execution
   */
  async mergeCoin(
    transaction: MergeCoinTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newMergeCoin(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   *
   * Serialize and sign a `SplitCoin` transaction and submit to the Fullnode
   * for execution
   */
  async splitCoin(
    transaction: SplitCoinTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newSplitCoin(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   * Serialize and sign a `MoveCall` transaction and submit to the Fullnode
   * for execution
   */
  async executeMoveCall(
    transaction: MoveCallTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newMoveCall(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   *
   * Serialize and sign a `Publish` transaction and submit to the Fullnode
   * for execution
   */
  async publish(
    transaction: PublishTransaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution'
  ): Promise<SuiExecuteTransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newPublish(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes, requestType);
  }

  /**
   * Returns the estimated gas cost for the transaction
   * @param tx The transaction to estimate the gas cost. When string it is assumed it's a serialized tx in base64
   * @returns total gas cost estimation
   * @throws whens fails to estimate the gas cost
   */
  async getGasCostEstimation(...args: Parameters<SignerWithProvider['dryRunTransaction']>) {
    const txEffects = await this.dryRunTransaction(...args);
    const gasEstimation = getTotalGasUsed(txEffects);
    if (typeof gasEstimation === 'undefined') {
      throw new Error('Failed to estimate the gas cost from transaction');
    }
    return gasEstimation;
  }
}
