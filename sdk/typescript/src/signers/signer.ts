// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { PublicKey } from '../cryptography/publickey';
import { Provider, TransactionResponse } from '../providers/provider';
import { Base64DataBuffer } from '../serialization/base64';
import {
  TransferTransaction,
  TxnDataSerializer,
} from './txn-data-serializers/txn-data-serializer';

///////////////////////////////
// Exported Types

/**
 * Pair of signature and corresponding public key
 */
export type SignaturePubkeyPair = {
  signature: Base64DataBuffer;
  pubKey: PublicKey;
};

///////////////////////////////
// Exported Abstracts
export abstract class Signer {
  readonly provider?: Provider;
  readonly serializer?: TxnDataSerializer;

  ///////////////////
  // Sub-classes MUST implement these

  // Returns the checksum address
  abstract getAddress(): Promise<string>;

  /**
   * Returns the signature for the data and the public key of the signer
   */
  abstract signData(data: Base64DataBuffer): Promise<SignaturePubkeyPair>;

  // Returns a new instance of the Signer, connected to provider.
  // This MAY throw if changing providers is not supported.
  abstract connect(provider: Provider): Signer;

  ///////////////////
  // Sub-classes MAY override these

  /**
   * Sign a transaction and submit to the Gateway for execution
   *
   * @param txBytes a Base64 string representation of BCS serialised TransactionData bytes
   */
  async signAndExecuteTransaction(
    txBytes: Base64DataBuffer
  ): Promise<TransactionResponse> {
    this._checkProvider('signAndExecuteTransaction');
    const sig = await this.signData(txBytes);
    return await this.provider!.executeTransaction({
      txBytes: txBytes.toString(),
      signature: sig.signature.toString(),
      pubKey: sig.pubKey.toString(),
    });
  }

  /**
   * Serialize and Sign a `Transfer` transaction and submit to the Gateway for execution
   */
  async transfer(
    transaction: TransferTransaction
  ): Promise<TransactionResponse> {
    this._checkProviderAndSerializer('transfer');
    const txBytes = await this.serializer!.new_transfer(transaction);
    return await this.signAndExecuteTransaction(txBytes);
  }

  ///////////////////
  // Sub-classes SHOULD leave these alone

  _checkProviderAndSerializer(operation?: string): void {
    this._checkProvider(operation);
    this._checkSerializer(operation);
  }

  _checkProvider(operation?: string): void {
    if (!this.provider) {
      throw new Error(`missing provider for ${operation || '_checkProvider'}`);
    }
  }

  _checkSerializer(operation?: string): void {
    if (!this.serializer) {
      throw new Error(
        `missing serializer for ${operation || '_checkSerializer'}`
      );
    }
  }
}
