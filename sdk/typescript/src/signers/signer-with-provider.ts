// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '../providers/json-rpc-provider';
import { Provider } from '../providers/provider';
import { VoidProvider } from '../providers/void-provider';
import { Base64DataBuffer } from '../serialization/base64';
import { SuiAddress, TransactionResponse } from '../types';
import { SignaturePubkeyPair, Signer } from './signer';
import { RpcTxnDataSerializer } from './txn-data-serializers/rpc-txn-data-serializer';
import {
  MoveCallTransaction,
  MergeCoinTransaction,
  SplitCoinTransaction,
  TransferObjectTransaction,
  TxnDataSerializer,
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

  constructor(provider?: Provider, serializer?: TxnDataSerializer) {
    this.provider = provider || new VoidProvider();
    let endpoint = '';
    if (this.provider instanceof JsonRpcProvider) {
      endpoint = this.provider.endpoint;
    }
    this.serializer = serializer || new RpcTxnDataSerializer(endpoint);
  }

  /**
   * Sign a transaction and submit to the Gateway for execution
   *
   * @param txBytes BCS serialised TransactionData bytes
   */
  async signAndExecuteTransaction(
    txBytes: Base64DataBuffer
  ): Promise<TransactionResponse> {
    const sig = await this.signData(txBytes);
    return await this.provider.executeTransaction(
      txBytes.toString(),
      sig.signature.toString(),
      sig.pubKey.toString()
    );
  }

  /**
   * Serialize and Sign a `TransferObject` transaction and submit to the Gateway for execution
   */
  async transferObject(
    transaction: TransferObjectTransaction
  ): Promise<TransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newTransferObject(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes);
  }

  /**
   * Serialize and Sign a `MergeCoin` transaction and submit to the Gateway for execution
   */
  async mergeCoin(
    transaction: MergeCoinTransaction
  ): Promise<TransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newMergeCoin(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes);
  }

  /**
   * Serialize and Sign a `SplitCoin` transaction and submit to the Gateway for execution
   */
  async splitCoin(
    transaction: SplitCoinTransaction
  ): Promise<TransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newSplitCoin(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes);
  }

  /**
   * Serialize and Sign a `MoveCall` transaction and submit to the Gateway for execution
   */
  async executeMoveCall(
    transaction: MoveCallTransaction
  ): Promise<TransactionResponse> {
    const signerAddress = await this.getAddress();
    const txBytes = await this.serializer.newMoveCall(
      signerAddress,
      transaction
    );
    return await this.signAndExecuteTransaction(txBytes);
  }
}
