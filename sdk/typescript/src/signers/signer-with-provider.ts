// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { fromB64, toB64 } from '@mysten/bcs';
import { Transaction } from '../builder';
import { TransactionDataBuilder } from '../builder/TransactionData';
import { SerializedSignature } from '../cryptography/signature';
import { JsonRpcProvider } from '../providers/json-rpc-provider';
import { HttpHeaders } from '../rpc/client';
import {
  ExecuteTransactionRequestType,
  FaucetResponse,
  getTotalGasUsedUpperBound,
  SuiAddress,
  DevInspectResults,
  DryRunTransactionResponse,
  SuiTransactionResponse,
  SuiTransactionResponseOptions,
} from '../types';
import { IntentScope, messageWithIntent } from '../utils/intent';
import { Signer } from './signer';
import { SignedTransaction, SignedMessage } from './types';

///////////////////////////////
// Exported Abstracts
export abstract class SignerWithProvider implements Signer {
  readonly provider: JsonRpcProvider;

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
  abstract connect(provider: JsonRpcProvider): SignerWithProvider;

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

  constructor(provider: JsonRpcProvider) {
    this.provider = provider;
  }

  /**
   * Sign a message using the keypair, with the `PersonalMessage` intent.
   */
  async signMessage(input: { message: Uint8Array }): Promise<SignedMessage> {
    const signature = await this.signData(
      messageWithIntent(IntentScope.PersonalMessage, input.message),
    );

    return {
      messageBytes: toB64(input.message),
      signature,
    };
  }

  /**
   * Sign a transaction.
   */
  async signTransaction(input: {
    transaction: Uint8Array | Transaction;
  }): Promise<SignedTransaction> {
    let transactionBytes;

    if (Transaction.is(input.transaction)) {
      input.transaction.setSender(await this.getAddress());
      transactionBytes = await input.transaction.build({
        provider: this.provider,
      });
    } else if (input.transaction instanceof Uint8Array) {
      transactionBytes = input.transaction;
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
   *
   * @param options specify which fields to return (e.g., transaction, effects, events, etc).
   * By default, only the transaction digest will be returned.
   * @param requestType WaitForEffectsCert or WaitForLocalExecution, see details in `ExecuteTransactionRequestType`.
   * Defaults to `WaitForLocalExecution` if options.show_effects or options.show_events is true
   */
  async signAndExecuteTransaction(input: {
    transaction: Uint8Array | Transaction;
    /** specify which fields to return (e.g., transaction, effects, events, etc). By default, only the transaction digest will be returned. */
    options?: SuiTransactionResponseOptions;
    /** `WaitForEffectsCert` or `WaitForLocalExecution`, see details in `ExecuteTransactionRequestType`.
     * Defaults to `WaitForLocalExecution` if options.show_effects or options.show_events is true
     */
    requestType?: ExecuteTransactionRequestType;
  }): Promise<SuiTransactionResponse> {
    const { transactionBytes, signature } = await this.signTransaction({
      transaction: input.transaction,
    });

    return await this.provider.executeTransaction({
      transaction: transactionBytes,
      signature,
      options: input.options,
      requestType: input.requestType,
    });
  }

  /**
   * Derive transaction digest from
   * @param tx BCS serialized transaction data or a `Transaction` object
   * @returns transaction digest
   */
  async getTransactionDigest(tx: Uint8Array | Transaction): Promise<string> {
    if (Transaction.is(tx)) {
      tx.setSender(await this.getAddress());
      return tx.getDigest({ provider: this.provider });
    } else if (tx instanceof Uint8Array) {
      return TransactionDataBuilder.getDigestFromBytes(tx);
    } else {
      throw new Error('Unknown transaction format.');
    }
  }

  /**
   * Runs the transaction in dev-inpsect mode. Which allows for nearly any
   * transaction (or Move call) with any arguments. Detailed results are
   * provided, including both the transaction effects and any return values.
   */
  async devInspectTransaction(
    input: Omit<
      Parameters<JsonRpcProvider['devInspectTransaction']>[0],
      'sender'
    >,
  ): Promise<DevInspectResults> {
    const address = await this.getAddress();
    return this.provider.devInspectTransaction({ sender: address, ...input });
  }

  /**
   * Dry run a transaction and return the result.
   */
  async dryRunTransaction(input: {
    transaction: Transaction | string | Uint8Array;
  }): Promise<DryRunTransactionResponse> {
    let dryRunTxBytes: Uint8Array;
    if (Transaction.is(input.transaction)) {
      input.transaction.setSender(await this.getAddress());
      dryRunTxBytes = await input.transaction.build({
        provider: this.provider,
      });
    } else if (typeof input.transaction === 'string') {
      dryRunTxBytes = fromB64(input.transaction);
    } else if (input.transaction instanceof Uint8Array) {
      dryRunTxBytes = input.transaction;
    } else {
      throw new Error('Unknown transaction format');
    }

    return this.provider.dryRunTransaction({ transaction: dryRunTxBytes });
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
