// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '../../serialization/base64';
import { ObjectId, SuiAddress, SuiJsonValue, TypeTag } from '../../types';

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

export interface PayTransaction {
  /**
   * use `provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual` to
   * derive a minimal set of coins with combined balance greater than or
   * equal to sent amounts
   */
  inputCoins: ObjectId[];
  recipients: SuiAddress[];
  amounts: number[];
  gasPayment?: ObjectId;
  gasBudget: number;
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
  typeArguments: string[] | TypeTag[];
  arguments: SuiJsonValue[];
  gasPayment?: ObjectId;
  gasBudget: number;
}

export type UnserializedSignableTransaction =
  | {
      kind: 'moveCall';
      data: MoveCallTransaction;
    }
  | {
      kind: 'transferSui';
      data: TransferSuiTransaction;
    }
  | {
      kind: 'transferObject';
      data: TransferObjectTransaction;
    }
  | {
      kind: 'mergeCoin';
      data: MergeCoinTransaction;
    }
  | {
      kind: 'splitCoin';
      data: SplitCoinTransaction;
    }
  | {
      kind: 'pay';
      data: PayTransaction;
    }
  | {
      kind: 'publish';
      data: PublishTransaction;
    };

/** A type that represents the possible transactions that can be signed: */
export type SignableTransaction =
  | UnserializedSignableTransaction
  | {
      kind: 'bytes';
      data: Uint8Array;
    };

export type SignableTransactionKind = SignableTransaction['kind'];
export type SignableTransactionData = SignableTransaction['data'];

/**
 * Transaction type used for publishing Move modules to the Sui.
 *
 * Use the util methods defined in [utils/publish.ts](../../utils/publish.ts)
 * to get `compiledModules` bytes by leveraging the sui
 * command line tool.
 *
 * ```
 * const { execSync } = require('child_process');
 * const modulesInBase64 = JSON.parse(execSync(
 *   `${cliPath} move build --dump-bytecode-as-base64 --path ${packagePath}`,
 *   { encoding: 'utf-8' }
 * ));
 *
 * // Include the following line if you are using `LocalTxnDataSerializer`, skip
 * // if you are using `RpcTxnDataSerializer`
 * // const modulesInBytes = modules.map((m) => Array.from(new Base64DataBuffer(m).getData()));
 * // ... publish logic ...
 * ```
 *
 */
export interface PublishTransaction {
  compiledModules: ArrayLike<string> | ArrayLike<ArrayLike<number>>;
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

  newPay(
    signerAddress: SuiAddress,
    txn: PayTransaction
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
