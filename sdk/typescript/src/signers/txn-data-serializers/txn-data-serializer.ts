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
  amount: number | null;
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
  /**
   * Transaction type used for publishing Move modules to the Sui.
   * Should be already compiled using `sui-move`, example:
   * ```
   * $ sui move build
   * $ cat build/project_name/bytecode_modules/module.mv
   * ```
   * In JS:
   *
   * ```
   * // If you are using `RpcTxnDataSerializer`,
   * let file = fs.readFileSync('./move/build/project_name/bytecode_modules/module.mv', 'base64');
   * let compiledModules = [file.toString()]
   *
   * // If you are using `LocalTxnDataSerializer`,
   * let file = fs.readFileSync('./move/build/project_name/bytecode_modules/module.mv');
   * let modules = [ Array.from(file) ];
   *
   * // ... publish logic ...
   * ```
   *
   * Each module should be represented as a sequence of bytes.
   */
  compiledModules: Iterable<string> | Iterable<Iterable<number>>;
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
