// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { builder, Transaction } from '../builder';
import { SerializedSignature } from '../cryptography/signature';
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
  DevInspectResults,
  DryRunTransactionResponse,
  SuiTransactionResponse,
} from '../types';
import { IntentScope, messageWithIntent } from '../utils/intent';
import { Signer } from './signer';
import { SignedTransaction, SignedMessage } from './types';

///////////////////////////////
// Exported Abstracts
export abstract class SignerWithProvider implements Signer {
  readonly provider: Provider;

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

  constructor(provider?: Provider) {
    this.provider = provider || new VoidProvider();
  }

  /**
   * Sign a message using the keypair, with the `PersonalMessage` intent.
   */
  async signMessage(message: Uint8Array): Promise<SignedMessage> {
    const signature = await this.signData(
      messageWithIntent(IntentScope.PersonalMessage, message),
    );

    return {
      messageBytes: toB64(message),
      signature,
    };
  }

  /**
   * Sign a transaction.
   */
  async signTransaction(
    transaction: Uint8Array | Transaction,
  ): Promise<SignedTransaction> {
    let transactionBytes;

    if (Transaction.is(transaction)) {
      transaction.setSender(await this.getAddress());
      transactionBytes = await transaction.build({ provider: this.provider });
    } else if (transaction instanceof Uint8Array) {
      transactionBytes = transaction;
    } else {
      throw new Error('Unknown transaction format');
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
    transaction: Uint8Array | Transaction,
    requestType: ExecuteTransactionRequestType = 'WaitForLocalExecution',
  ): Promise<SuiTransactionResponse> {
    const { transactionBytes, signature } = await this.signTransaction(
      transaction,
    );

    return await this.provider.executeTransaction(
      transactionBytes,
      signature,
      requestType,
    );
  }

  async getTransactionDigest(tx: Uint8Array | Transaction): Promise<string> {
    let txBytes: Uint8Array;
    if (Transaction.is(tx)) {
      tx.setSender(await this.getAddress());
      txBytes = await tx.build({ provider: this.provider });
    } else if (tx instanceof Uint8Array) {
      txBytes = tx;
    } else {
      throw new Error('Unknown transaction format.');
    }

    // TODO: Why do we deserialize, then immedietly re-serialize the transaction data here?
    // Probably can improve this with some `Transaction` helpers to build just transaction data.
    const data = deserializeTransactionBytesToTransactionData(builder, txBytes);
    return generateTransactionDigest(data, builder);
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
    tx: Transaction | string | Uint8Array,
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
    tx: Transaction | string | Uint8Array,
  ): Promise<DryRunTransactionResponse> {
    let dryRunTxBytes: Uint8Array;
    if (Transaction.is(tx)) {
      tx.setSender(await this.getAddress());
      dryRunTxBytes = await tx.build({ provider: this.provider });
    } else if (typeof tx === 'string') {
      dryRunTxBytes = fromB64(tx);
    } else if (tx instanceof Uint8Array) {
      dryRunTxBytes = tx;
    } else {
      throw new Error('Unknown transaction format');
    }
    return this.provider.dryRunTransaction(dryRunTxBytes);
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
    const gasEstimation = getTotalGasUsedUpperBound(txEffects.effects);
    if (typeof gasEstimation === 'undefined') {
      throw new Error('Failed to estimate the gas cost from transaction');
    }
    return gasEstimation;
  }
}
