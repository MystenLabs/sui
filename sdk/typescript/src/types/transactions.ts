// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ObjectOwner, SuiAddress, TransactionDigest } from './common';
import { isTransactionEffects } from './index.guard';
import { ObjectId, SuiMovePackage, SuiObject, SuiObjectRef } from './objects';

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

export type Pay = {
  coins: SuiObjectRef[];
  recipients: SuiAddress[];
  amounts: number[];
};

export type PaySui = {
  coins: SuiObjectRef[];
  recipients: SuiAddress[];
  amounts: number[];
};

export type PayAllSui = {
  coins: SuiObjectRef[];
  recipient: SuiAddress;
};

export type ExecuteTransactionRequestType =
  | 'ImmediateReturn'
  | 'WaitForTxCert'
  | 'WaitForEffectsCert'
  | 'WaitForLocalExecution';

export type TransactionKindName =
  | 'TransferObject'
  | 'Publish'
  | 'Call'
  | 'TransferSui'
  | 'ChangeEpoch'
  | 'Pay'
  | 'PaySui'
  | 'PayAllSui';

export type SuiTransactionKind =
  | { TransferObject: TransferObject }
  | { Publish: SuiMovePackage }
  | { Call: MoveCall }
  | { TransferSui: SuiTransferSui }
  | { ChangeEpoch: SuiChangeEpoch }
  | { Pay: Pay }
  | { PaySui: PaySui }
  | { PayAllSui: PayAllSui };
export type SuiTransactionData = {
  transactions: SuiTransactionKind[];
  sender: SuiAddress;
  gasPayment: SuiObjectRef;
  gasBudget: number;
};

// TODO: support u64
export type EpochId = number;
export type GenericAuthoritySignature =
  | AuthoritySignature[]
  | AuthoritySignature;

export type AuthorityQuorumSignInfo = {
  epoch: EpochId;
  signature: GenericAuthoritySignature;
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

// TODO: this is likely to go away after https://github.com/MystenLabs/sui/issues/4207
export type SuiCertifiedTransactionEffects = {
  effects: TransactionEffects;
};

export type SuiExecuteTransactionResponse =
  | {
      ImmediateReturn: {
        tx_digest: string;
      };
    }
  | { TxCert: { certificate: CertifiedTransaction } }
  | {
      EffectsCert: {
        certificate: CertifiedTransaction;
        effects: SuiCertifiedTransactionEffects;
      };
    };

export type GatewayTxSeqNumber = number;

export type GetTxnDigestsResponse = TransactionDigest[];

export type PaginatedTransactionDigests = {
  data: TransactionDigest[];
  nextCursor: TransactionDigest | null;
};

export type TransactionQuery =
  | 'All'
  | {
      MoveFunction: {
        package: ObjectId;
        module: string | null;
        function: string | null;
      };
    }
  | { InputObject: ObjectId }
  | { MutatedObject: ObjectId }
  | { FromAddress: SuiAddress }
  | { ToAddress: SuiAddress };

export type MoveCall = {
  package: SuiObjectRef;
  module: string;
  function: string;
  typeArguments?: string[];
  arguments?: SuiJsonValue[];
};

export type SuiJsonValue = boolean | number | string | Array<SuiJsonValue>;

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

export function getCertifiedTransaction(
  tx: SuiTransactionResponse | SuiExecuteTransactionResponse
): CertifiedTransaction | undefined {
  if ('certificate' in tx) {
    return tx.certificate;
  } else if ('TxCert' in tx) {
    return tx.TxCert.certificate;
  } else if ('EffectsCert' in tx) {
    return tx.EffectsCert.certificate;
  }
  return undefined;
}

export function getTransactionDigest(
  tx:
    | CertifiedTransaction
    | SuiTransactionResponse
    | SuiExecuteTransactionResponse
): TransactionDigest {
  if ('ImmediateReturn' in tx) {
    return tx.ImmediateReturn.tx_digest;
  }
  if ('transactionDigest' in tx) {
    return tx.transactionDigest;
  }
  const ctxn = getCertifiedTransaction(tx)!;
  return ctxn.transactionDigest;
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

export function getPayTransaction(data: SuiTransactionKind): Pay | undefined {
  return 'Pay' in data ? data.Pay : undefined;
}

export function getPaySuiTransaction(
  data: SuiTransactionKind
): PaySui | undefined {
  return 'PaySui' in data ? data.PaySui : undefined;
}

export function getPayAllSuiTransaction(
  data: SuiTransactionKind
): PayAllSui | undefined {
  return 'PayAllSui' in data ? data.PayAllSui : undefined;
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

export function getTransferSuiAmount(data: SuiTransactionKind): bigint | null {
  return 'TransferSui' in data && data.TransferSui.amount
    ? BigInt(data.TransferSui.amount)
    : null;
}

export function getTransactionKindName(
  data: SuiTransactionKind
): TransactionKindName {
  return Object.keys(data)[0] as TransactionKindName;
}

/* ----------------------------- ExecutionStatus ---------------------------- */

export function getExecutionStatusType(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse
): ExecutionStatusType | undefined {
  return getExecutionStatus(data)?.status;
}

export function getExecutionStatus(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse
): ExecutionStatus | undefined {
  return getTransactionEffects(data)?.status;
}

export function getExecutionStatusError(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse
): string | undefined {
  return getExecutionStatus(data)?.error;
}

export function getExecutionStatusGasSummary(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse | TransactionEffects
): GasCostSummary | undefined {
  if (isTransactionEffects(data)) {
    return data.gasUsed;
  }
  return getTransactionEffects(data)?.gasUsed;
}

export function getTotalGasUsed(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse | TransactionEffects
): number | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? gasSummary.computationCost +
        gasSummary.storageCost -
        gasSummary.storageRebate
    : undefined;
}

export function getTransactionEffects(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse
): TransactionEffects | undefined {
  if ('effects' in data) {
    return data.effects;
  }
  return 'EffectsCert' in data ? data.EffectsCert.effects.effects : undefined;
}

/* ---------------------------- Transaction Effects --------------------------- */

export function getEvents(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse
): any {
  return getTransactionEffects(data)?.events;
}

export function getCreatedObjects(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse
): OwnedObjectRef[] | undefined {
  return getTransactionEffects(data)?.created;
}

/* --------------------------- TransactionResponse -------------------------- */

export function getTimestampFromTransactionResponse(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse
): number | undefined {
  return 'timestamp_ms' in data ? data.timestamp_ms ?? undefined : undefined;
}

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

/**
 * Get the newly created coin refs after a split.
 */
export function getNewlyCreatedCoinRefsAfterSplit(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse
): SuiObjectRef[] | undefined {
  if ('EffectsCert' in data) {
    const effects = data.EffectsCert.effects.effects;
    return effects.created?.map((c) => c.reference);
  }
  return undefined;
}
