// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner, SuiAddress, TransactionDigest } from './common';
import { SuiMovePackage, SuiObject, SuiObjectRef } from './objects';

export type TransferCoin = {
  recipient: SuiAddress;
  objectRef: SuiObjectRef;
};
export type RawAuthoritySignInfo = [AuthorityName, AuthoritySignature];

export type TransactionKindName = 'TransferCoin' | 'Publish' | 'Call';
export type SuiTransactionKind =
  | { TransferCoin: TransferCoin }
  | { Publish: SuiMovePackage }
  | { Call: MoveCall };
export type TransactionData = {
  transactions: SuiTransactionKind[];
  sender: SuiAddress;
  gasPayment: SuiObjectRef;
  gasBudget: number;
};

// TODO: support u64
export type EpochId = number;

export type AuthorityQuorumSignInfo = {
  epoch: EpochId;
  signatures: RawAuthoritySignInfo[];
};

export type CertifiedTransaction = {
  transactionDigest: TransactionDigest;
  data: TransactionData;
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
  gas_cost: GasCostSummary;
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
  events?: Event[];
  /** The set of transaction digests this transaction depends on */
  dependencies?: TransactionDigest[];
};

export type TransactionEffectsResponse = {
  certificate: CertifiedTransaction;
  effects: TransactionEffects;
};

export type GatewayTxSeqNumber = number;

export type GetTxnDigestsResponse = [GatewayTxSeqNumber, TransactionDigest][];

export type Event = {
  type_: string;
  contents: any;
};

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

export type MergeCoinResponse = {
  certificate: CertifiedTransaction;
  updatedCoin: SuiObject;
  updatedGas: SuiObject;
};

export type SplitCoinResponse = {
  certificate: CertifiedTransaction;
  updatedCoin: SuiObject;
  newCoins: SuiObject[];
  updatedGas: SuiObject;
};

export type TransactionResponse =
  | {
      EffectResponse: TransactionEffectsResponse;
      // TODO: Add Publish, MergeCoin Response
    }
  | {
      SplitCoinResponse: SplitCoinResponse;
    }
  | {
      MergeCoinResponse: MergeCoinResponse;
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

export function getTransactionData(tx: CertifiedTransaction): TransactionData {
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

export function getTransferCoinTransaction(
  data: SuiTransactionKind
): TransferCoin | undefined {
  return 'TransferCoin' in data ? data.TransferCoin : undefined;
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

export function getTransactions(
  data: CertifiedTransaction
): SuiTransactionKind[] {
  return data.data.transactions;
}

export function getTransactionKindName(
  data: SuiTransactionKind
): TransactionKindName {
  return Object.keys(data)[0] as TransactionKindName;
}

/* ----------------------------- ExecutionStatus ---------------------------- */

export function getExecutionStatusType(
  data: TransactionEffectsResponse
): ExecutionStatusType {
  return getExecutionStatus(data).status;
}

export function getExecutionStatus(
  data: TransactionEffectsResponse
): ExecutionStatus {
  return data.effects.status;
}

export function getExecutionStatusError(
  data: TransactionEffectsResponse
): string | undefined {
  return getExecutionStatus(data).error;
}

export function getExecutionStatusGasSummary(
  data: TransactionEffectsResponse
): GasCostSummary {
  return getExecutionStatus(data).gas_cost;
}

export function getTotalGasUsed(data: TransactionEffectsResponse): number {
  const gasSummary = getExecutionStatusGasSummary(data);
  return (
    gasSummary.computationCost +
    gasSummary.storageCost -
    gasSummary.storageRebate
  );
}
