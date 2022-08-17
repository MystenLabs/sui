// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';
import {
  CallArg,
  ObjectId,
  SuiAddress,
  SuiJsonValue,
  TypeTag,
} from '../../types';

///////////////////////////////
// Exported Types
export interface TransferObjectTransaction {
  objectId: ObjectId;
  gasPayment?: ObjectId;
  gasBudget: number;
  recipient: SuiAddress;
}

export interface TransferSuiTransaction {
  suiObjectId: ObjectId;
  gasBudget: number;
  recipient: SuiAddress;
  amount?: number;
}

export interface MergeCoinTransaction {
  primaryCoin: ObjectId;
  coinToMerge: ObjectId;
  gasPayment?: ObjectId;
  gasBudget: number;
}

export interface SplitCoinTransaction {
  coinObjectId: ObjectId;
  splitAmounts: number[];
  gasPayment?: ObjectId;
  gasBudget: number;
}

export interface MoveCallTransaction {
  packageObjectId: ObjectId;
  module: string;
  function: string;
  /**
   * Usage: pass in string[] if you use RpcTxnDataSerializer,
   * Otherwise you need to pass in TypeTag[]. We will remove
   * RpcTxnDataSerializer soon.
   */
  typeArguments: string[] | TypeTag[];
  /**
   * Usage: pass in SuiJsonValue[] if you use RpcTxnDataSerializer,
   * Otherwise you need to pass in CallArg[].
   */
  arguments: SuiJsonValue[] | CallArg[];
  gasPayment?: ObjectId;
  gasBudget: number;
}

export interface PublishTransaction {
  compiledModules: string[];
  gasPayment?: ObjectId;
  gasBudget: number;
}

///////////////////////////////
// Exported Abstracts
/**
 * Serializes a transaction to a string that can be signed by a `Signer`.
 */
export interface TxnDataSerializer {
  newTransferObject(
    signerAddress: SuiAddress,
    txn: TransferObjectTransaction
  ): Promise<Base64DataBuffer>;

  newTransferSui(
    signerAddress: SuiAddress,
    txn: TransferSuiTransaction
  ): Promise<Base64DataBuffer>;

  newMoveCall(
    signerAddress: SuiAddress,
    txn: MoveCallTransaction
  ): Promise<Base64DataBuffer>;

  newMergeCoin(
    signerAddress: SuiAddress,
    txn: MergeCoinTransaction
  ): Promise<Base64DataBuffer>;

  newSplitCoin(
    signerAddress: SuiAddress,
    txn: SplitCoinTransaction
  ): Promise<Base64DataBuffer>;

  newPublish(
    signerAddress: SuiAddress,
    txn: PublishTransaction
  ): Promise<Base64DataBuffer>;
}
