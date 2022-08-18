// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner, SuiAddress, TransactionDigest } from './common';
import { SuiMovePackage, SuiObject, SuiObjectRef } from './objects';

import BN from 'bn.js';

export type TransferObject = {
  recipient: SuiAddress;
  objectRef: SuiObjectRef;
};

export type SuiTransferSui = {
  recipient: SuiAddress;
  amount: number | null;
};

export type SuiChangeEpoch = {
  epoch: EpochId;
  storage_charge: number;
  computation_charge: number;
};

export type TransactionKindName =
  | 'TransferObject'
  | 'Publish'
  | 'Call'
  | 'TransferSui'
  | 'ChangeEpoch';
export type SuiTransactionKind =
  | { TransferObject: TransferObject }
  | { Publish: SuiMovePackage }
  | { Call: MoveCall }
  | { TransferSui: SuiTransferSui }
  | { ChangeEpoch: SuiChangeEpoch };
export type SuiTransactionData = {
  transactions: SuiTransactionKind[];
  sender: SuiAddress;
  gasPayment: SuiObjectRef;
  gasBudget: number;
};

// TODO: support u64
export type EpochId = number;

export type AuthorityQuorumSignInfo = {
  epoch: EpochId;
  signature: AuthoritySignature[];
};

export type CertifiedTransaction = {
  transactionDigest: TransactionDigest;
  data: SuiTransactionData;
  txSignature: string;
  authSignInfo: AuthorityQuorumSignInfo;
};

export type GasCostSummary = {
  computationCost: number;
  storageCost: number;
  storageRebate: number;
};

export type ExecutionStatusType = 'success' | 'failure';
export type ExecutionStatus = {
  status: ExecutionStatusType;
  error?: string;
};

// TODO: change the tuple to struct from the server end
export type OwnedObjectRef = {
  owner: ObjectOwner;
  reference: SuiObjectRef;
};

export type TransactionEffects = {
  /** The status of the execution */
  status: ExecutionStatus;
  gasUsed: GasCostSummary;
  /** The object references of the shared objects used in this transaction. Empty if no shared objects were used. */
  sharedObjects?: SuiObjectRef[];
  /** The transaction digest */
  transactionDigest: TransactionDigest;
  /** ObjectRef and owner of new objects created */
  created?: OwnedObjectRef[];
  /** ObjectRef and owner of mutated objects, including gas object */
  mutated?: OwnedObjectRef[];
  /**
   * ObjectRef and owner of objects that are unwrapped in this transaction.
   * Unwrapped objects are objects that were wrapped into other objects in the past,
   * and just got extracted out.
   */
  unwrapped?: OwnedObjectRef[];
  /** Object Refs of objects now deleted (the old refs) */
  deleted?: SuiObjectRef[];
  /** Object refs of objects now wrapped in other objects */
  wrapped?: SuiObjectRef[];
  /**
   * The updated gas object reference. Have a dedicated field for convenient access.
   * It's also included in mutated.
   */
  gasObject: OwnedObjectRef;
  /** The events emitted during execution. Note that only successful transactions emit events */
  // TODO: properly define type when this is being used
  events?: any[];
  /** The set of transaction digests this transaction depends on */
  dependencies?: TransactionDigest[];
};

export type SuiTransactionResponse = {
  certificate: CertifiedTransaction;
  effects: TransactionEffects;
  timestamp_ms: number | null;
  parsed_data: SuiParsedTransactionResponse | null;
};

export type GatewayTxSeqNumber = number;

export type GetTxnDigestsResponse = [GatewayTxSeqNumber, TransactionDigest][];

export type MoveCall = {
  package: SuiObjectRef;
  module: string;
  function: string;
  typeArguments?: string[];
  arguments?: SuiJsonValue[];
};

export type SuiJsonValue =
  | boolean
  | number
  | string
  | Array<boolean | number | string>;

export type EmptySignInfo = object;
export type AuthorityName = string;
export type AuthoritySignature = string;

export type TransactionBytes = {
  txBytes: string;
  gas: SuiObjectRef;
  // TODO: Add input_objects field
};

export type SuiParsedMergeCoinResponse = {
  updatedCoin: SuiObject;
  updatedGas: SuiObject;
};

export type SuiParsedSplitCoinResponse = {
  updatedCoin: SuiObject;
  newCoins: SuiObject[];
  updatedGas: SuiObject;
};

export type SuiParsedPublishResponse = {
  createdObjects: SuiObject[];
  package: SuiPackage;
  updatedGas: SuiObject;
};

export type SuiPackage = {
  digest: string;
  objectId: string;
  version: number;
};

export type SuiParsedTransactionResponse =
  | {
      SplitCoin: SuiParsedSplitCoinResponse;
    }
  | {
      MergeCoin: SuiParsedMergeCoinResponse;
    }
  | {
      Publish: SuiParsedPublishResponse;
    };

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

/* ---------------------------------- CertifiedTransaction --------------------------------- */
export function getTransactionDigest(
  tx: CertifiedTransaction
): TransactionDigest {
  return tx.transactionDigest;
}

export function getTransactionSignature(tx: CertifiedTransaction): string {
  return tx.txSignature;
}

export function getTransactionAuthorityQuorumSignInfo(
  tx: CertifiedTransaction
): AuthorityQuorumSignInfo {
  return tx.authSignInfo;
}

export function getTransactionData(
  tx: CertifiedTransaction
): SuiTransactionData {
  return tx.data;
}

/* ----------------------------- TransactionData ---------------------------- */

export function getTransactionSender(tx: CertifiedTransaction): SuiAddress {
  return tx.data.sender;
}

export function getTransactionGasObject(
  tx: CertifiedTransaction
): SuiObjectRef {
  return tx.data.gasPayment;
}

export function getTransactionGasBudget(tx: CertifiedTransaction): number {
  return tx.data.gasBudget;
}

export function getTransferObjectTransaction(
  data: SuiTransactionKind
): TransferObject | undefined {
  return 'TransferObject' in data ? data.TransferObject : undefined;
}

export function getPublishTransaction(
  data: SuiTransactionKind
): SuiMovePackage | undefined {
  return 'Publish' in data ? data.Publish : undefined;
}

export function getMoveCallTransaction(
  data: SuiTransactionKind
): MoveCall | undefined {
  return 'Call' in data ? data.Call : undefined;
}

export function getTransferSuiTransaction(
  data: SuiTransactionKind
): SuiTransferSui | undefined {
  return 'TransferSui' in data ? data.TransferSui : undefined;
}

export function getChangeEpochTransaction(
  data: SuiTransactionKind
): SuiChangeEpoch | undefined {
  return 'ChangeEpoch' in data ? data.ChangeEpoch : undefined;
}

export function getTransactions(
  data: CertifiedTransaction
): SuiTransactionKind[] {
  return data.data.transactions;
}

export function getTransferSuiAmount(
  data: SuiTransactionKind
): BN | null {
  return ("TransferSui" in data && data.TransferSui.amount) ? new BN.BN(data.TransferSui.amount, 10) : null; 
}

export function getTransactionKindName(
  data: SuiTransactionKind
): TransactionKindName {
  return Object.keys(data)[0] as TransactionKindName;
}

/* ----------------------------- ExecutionStatus ---------------------------- */

export function getExecutionStatusType(
  data: SuiTransactionResponse
): ExecutionStatusType {
  return getExecutionStatus(data).status;
}

export function getExecutionStatus(
  data: SuiTransactionResponse
): ExecutionStatus {
  return data.effects().status;
}

export function getExecutionStatusError(
  data: SuiTransactionResponse
): string | undefined {
  return getExecutionStatus(data).error;
}

export function getExecutionStatusGasSummary(
  data: SuiTransactionResponse
): GasCostSummary {
  return data.effects().gasUsed;
}

export function getTotalGasUsed(data: SuiTransactionResponse): number {
  const gasSummary = getExecutionStatusGasSummary(data);
  return (
    gasSummary.computationCost +
    gasSummary.storageCost -
    gasSummary.storageRebate
  );
}

/* --------------------------- TransactionResponse -------------------------- */

export function getParsedSplitCoinResponse(
  data: SuiTransactionResponse
): SuiParsedSplitCoinResponse | undefined {
  const parsed = data.parsed_data;
  return parsed && 'SplitCoin' in parsed ? parsed.SplitCoin : undefined;
}

export function getParsedMergeCoinResponse(
  data: SuiTransactionResponse
): SuiParsedMergeCoinResponse | undefined {
  const parsed = data.parsed_data;
  return parsed && 'MergeCoin' in parsed ? parsed.MergeCoin : undefined;
}

export function getParsedPublishResponse(
  data: SuiTransactionResponse
): SuiParsedPublishResponse | undefined {
  const parsed = data.parsed_data;
  return parsed && 'Publish' in parsed ? parsed.Publish : undefined;
}

/**
 * Get the updated coin after a merge.
 * @param data the response for executing a merge coin transaction
 * @returns the updated state of the primary coin after the merge
 */
export function getCoinAfterMerge(
  data: SuiTransactionResponse
): SuiObject | undefined {
  return getParsedMergeCoinResponse(data)?.updatedCoin;
}

/**
 * Get the updated coin after a split.
 * @param data the response for executing a Split coin transaction
 * @returns the updated state of the original coin object used for the split
 */
export function getCoinAfterSplit(
  data: SuiTransactionResponse
): SuiObject | undefined {
  return getParsedSplitCoinResponse(data)?.updatedCoin;
}

/**
 * Get the newly created coin after a split.
 * @param data the response for executing a Split coin transaction
 * @returns the updated state of the original coin object used for the split
 */
export function getNewlyCreatedCoinsAfterSplit(
  data: SuiTransactionResponse
): SuiObject[] | undefined {
  return getParsedSplitCoinResponse(data)?.newCoins;
}
