// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  is,
  array,
  Infer,
  literal,
  number,
  object,
  optional,
  string,
  union,
  unknown,
  boolean,
  tuple,
} from 'superstruct';
import { SuiEvent } from './events';
import { SuiMovePackage, SuiObject, SuiObjectRef } from './objects';
import {
  ObjectId,
  ObjectOwner,
  SuiAddress,
  SuiJsonValue,
  TransactionDigest,
} from './common';

// TODO: support u64
export const EpochId = number();

export const TransferObject = object({
  recipient: SuiAddress,
  objectRef: SuiObjectRef,
});
export type TransferObject = Infer<typeof TransferObject>;

export const SuiTransferSui = object({
  recipient: SuiAddress,
  amount: union([number(), literal(null)]),
});
export type SuiTransferSui = Infer<typeof SuiTransferSui>;

export const SuiChangeEpoch = object({
  epoch: EpochId,
  storage_charge: number(),
  computation_charge: number(),
  // TODO: Make non-optional after v0.26.0 lands everywhere
  storage_rebate: optional(number()),
  epoch_start_timestamp_ms: optional(number()),
});
export type SuiChangeEpoch = Infer<typeof SuiChangeEpoch>;

export const SuiConsensusCommitPrologue = object({
  checkpoint_start_timestamp_ms: number(),
});
export type SuiConsensusCommitPrologue = Infer<
  typeof SuiConsensusCommitPrologue
>;

export const Pay = object({
  coins: array(SuiObjectRef),
  recipients: array(SuiAddress),
  amounts: array(number()),
});
export type Pay = Infer<typeof Pay>;

export const PaySui = object({
  coins: array(SuiObjectRef),
  recipients: array(SuiAddress),
  amounts: array(number()),
});
export type PaySui = Infer<typeof PaySui>;

export const PayAllSui = object({
  coins: array(SuiObjectRef),
  recipient: SuiAddress,
});
export type PayAllSui = Infer<typeof PayAllSui>;

export const MoveCall = object({
  // TODO: Simplify once 0.24.0 lands
  package: union([string(), SuiObjectRef]),
  module: string(),
  function: string(),
  typeArguments: optional(array(string())),
  arguments: array(SuiJsonValue),
});
export type MoveCall = Infer<typeof MoveCall>;

export const Genesis = object({
  objects: array(ObjectId),
});
export type Genesis = Infer<typeof Genesis>;

export type ExecuteTransactionRequestType =
  | 'WaitForEffectsCert'
  | 'WaitForLocalExecution';

export type TransactionKindName =
  | 'TransferObject'
  | 'Publish'
  | 'Call'
  | 'TransferSui'
  | 'ChangeEpoch'
  | 'ConsensusCommitPrologue'
  | 'Pay'
  | 'PaySui'
  | 'PayAllSui'
  | 'Genesis';

export const SuiTransactionKind = union([
  object({ TransferObject: TransferObject }),
  object({ Publish: SuiMovePackage }),
  object({ Call: MoveCall }),
  object({ TransferSui: SuiTransferSui }),
  object({ ChangeEpoch: SuiChangeEpoch }),
  object({ ConsensusCommitPrologue: SuiConsensusCommitPrologue }),
  object({ Pay: Pay }),
  object({ PaySui: PaySui }),
  object({ PayAllSui: PayAllSui }),
  object({ Genesis: Genesis }),
]);
export type SuiTransactionKind = Infer<typeof SuiTransactionKind>;

export const SuiTransactionData = object({
  transactions: array(SuiTransactionKind),
  sender: SuiAddress,
  gasPayment: SuiObjectRef,
  // TODO: remove optional after 0.21.0 is released
  gasPrice: optional(number()),
  gasBudget: number(),
});
export type SuiTransactionData = Infer<typeof SuiTransactionData>;

export const AuthoritySignature = string();
export const GenericAuthoritySignature = union([
  AuthoritySignature,
  array(AuthoritySignature),
]);

export const AuthorityQuorumSignInfo = object({
  epoch: EpochId,
  signature: GenericAuthoritySignature,
  signers_map: array(number()),
});
export type AuthorityQuorumSignInfo = Infer<typeof AuthorityQuorumSignInfo>;

export const CertifiedTransaction = object({
  transactionDigest: TransactionDigest,
  data: SuiTransactionData,
  txSignature: string(),
  authSignInfo: AuthorityQuorumSignInfo,
});
export type CertifiedTransaction = Infer<typeof CertifiedTransaction>;

export const GasCostSummary = object({
  computationCost: number(),
  storageCost: number(),
  storageRebate: number(),
});
export type GasCostSummary = Infer<typeof GasCostSummary>;

export const ExecutionStatusType = union([
  literal('success'),
  literal('failure'),
]);
export type ExecutionStatusType = Infer<typeof ExecutionStatusType>;

export const ExecutionStatus = object({
  status: ExecutionStatusType,
  error: optional(string()),
});
export type ExecutionStatus = Infer<typeof ExecutionStatus>;

// TODO: change the tuple to struct from the server end
export const OwnedObjectRef = object({
  owner: ObjectOwner,
  reference: SuiObjectRef,
});
export type OwnedObjectRef = Infer<typeof OwnedObjectRef>;

export const TransactionEffects = object({
  /** The status of the execution */
  status: ExecutionStatus,
  /**
   * The epoch when this transaction was executed
   * TODO: Changed it to non-optional once this is stable.
   * */
  executedEpoch: optional(EpochId),
  gasUsed: GasCostSummary,
  /** The object references of the shared objects used in this transaction. Empty if no shared objects were used. */
  sharedObjects: optional(array(SuiObjectRef)),
  /** The transaction digest */
  transactionDigest: TransactionDigest,
  /** ObjectRef and owner of new objects created */
  created: optional(array(OwnedObjectRef)),
  /** ObjectRef and owner of mutated objects, including gas object */
  mutated: optional(array(OwnedObjectRef)),
  /**
   * ObjectRef and owner of objects that are unwrapped in this transaction.
   * Unwrapped objects are objects that were wrapped into other objects in the past,
   * and just got extracted out.
   */
  unwrapped: optional(array(OwnedObjectRef)),
  /** Object Refs of objects now deleted (the old refs) */
  deleted: optional(array(SuiObjectRef)),
  /** Object refs of objects now wrapped in other objects */
  wrapped: optional(array(SuiObjectRef)),
  /**
   * The updated gas object reference. Have a dedicated field for convenient access.
   * It's also included in mutated.
   */
  gasObject: OwnedObjectRef,
  /** The events emitted during execution. Note that only successful transactions emit events */
  events: optional(array(SuiEvent)),
  /** The set of transaction digests this transaction depends on */
  dependencies: optional(array(TransactionDigest)),
});
export type TransactionEffects = Infer<typeof TransactionEffects>;

const ReturnValueType = tuple([array(number()), string()]);
const MutableReferenceOutputType = tuple([number(), array(number()), string()]);
const ExecutionResultType = object({
  mutableReferenceOutputs: optional(array(MutableReferenceOutputType)),
  returnValues: optional(array(ReturnValueType)),
});
const DevInspectResultTupleType = tuple([number(), ExecutionResultType]);

const DevInspectResultsType = union([
  object({ Ok: array(DevInspectResultTupleType) }),
  object({ Err: string() }),
]);

export const DevInspectResults = object({
  effects: TransactionEffects,
  results: DevInspectResultsType,
});
export type DevInspectResults = Infer<typeof DevInspectResults>;

export const SuiTransactionAuthSignersResponse = object({
  signers: array(string()),
});
export type SuiTransactionAuthSignersResponse = Infer<
  typeof SuiTransactionAuthSignersResponse
>;

// TODO: this is likely to go away after https://github.com/MystenLabs/sui/issues/4207
export const SuiCertifiedTransactionEffects = object({
  transactionEffectsDigest: string(),
  authSignInfo: AuthorityQuorumSignInfo,
  effects: TransactionEffects,
});

export const SuiEffectsFinalityInfo = union([
  object({ certified: AuthorityQuorumSignInfo }),
  object({ checkpointed: tuple([number(), number()]) }),
]);
export type SuiEffectsFinalityInfo = Infer<typeof SuiEffectsFinalityInfo>;

export const SuiFinalizedEffects = object({
  transactionEffectsDigest: string(),
  effects: TransactionEffects,
  finalityInfo: SuiEffectsFinalityInfo,
});
export type SuiFinalizedEffects = Infer<typeof SuiFinalizedEffects>;

export const SuiExecuteTransactionResponse = union([
  // TODO: remove after devnet 0.25.0(or 0.24.0) is released
  object({
    EffectsCert: object({
      certificate: CertifiedTransaction,
      effects: SuiCertifiedTransactionEffects,
      confirmed_local_execution: boolean(),
    }),
  }),
  object({
    certificate: optional(CertifiedTransaction),
    effects: SuiFinalizedEffects,
    confirmed_local_execution: boolean(),
  }),
]);
export type SuiExecuteTransactionResponse = Infer<
  typeof SuiExecuteTransactionResponse
>;

export type GatewayTxSeqNumber = number;

export const GetTxnDigestsResponse = array(TransactionDigest);
export type GetTxnDigestsResponse = Infer<typeof GetTxnDigestsResponse>;

export const PaginatedTransactionDigests = object({
  data: array(TransactionDigest),
  nextCursor: union([TransactionDigest, literal(null)]),
});
export type PaginatedTransactionDigests = Infer<
  typeof PaginatedTransactionDigests
>;

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

export type EmptySignInfo = object;
export type AuthorityName = Infer<typeof AuthorityName>;
export const AuthorityName = string();

export const TransactionBytes = object({
  txBytes: string(),
  gas: SuiObjectRef,
  // TODO: Type input_objects field
  inputObjects: unknown(),
});

export const SuiParsedMergeCoinResponse = object({
  updatedCoin: SuiObject,
  updatedGas: SuiObject,
});
export type SuiParsedMergeCoinResponse = Infer<
  typeof SuiParsedMergeCoinResponse
>;

export const SuiParsedSplitCoinResponse = object({
  updatedCoin: SuiObject,
  newCoins: array(SuiObject),
  updatedGas: SuiObject,
});
export type SuiParsedSplitCoinResponse = Infer<
  typeof SuiParsedSplitCoinResponse
>;

export const SuiPackage = object({
  digest: string(),
  objectId: string(),
  version: number(),
});

export const SuiParsedPublishResponse = object({
  createdObjects: array(SuiObject),
  package: SuiPackage,
  updatedGas: SuiObject,
});
export type SuiParsedPublishResponse = Infer<typeof SuiParsedPublishResponse>;

export const SuiParsedTransactionResponse = union([
  object({ SplitCoin: SuiParsedSplitCoinResponse }),
  object({ MergeCoin: SuiParsedMergeCoinResponse }),
  object({ Publish: SuiParsedPublishResponse }),
]);
export type SuiParsedTransactionResponse = Infer<
  typeof SuiParsedTransactionResponse
>;

export const SuiTransactionResponse = object({
  certificate: CertifiedTransaction,
  effects: TransactionEffects,
  timestamp_ms: union([number(), literal(null)]),
  parsed_data: union([SuiParsedTransactionResponse, literal(null)]),
});
export type SuiTransactionResponse = Infer<typeof SuiTransactionResponse>;

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

/* ---------------------------------- CertifiedTransaction --------------------------------- */

export function getCertifiedTransaction(
  tx: SuiTransactionResponse | SuiExecuteTransactionResponse,
): CertifiedTransaction | undefined {
  if ('certificate' in tx) {
    return tx.certificate;
  } else if ('EffectsCert' in tx) {
    return tx.EffectsCert.certificate;
  }
  return undefined;
}

export function getTransactionDigest(
  tx:
    | CertifiedTransaction
    | SuiTransactionResponse
    | SuiExecuteTransactionResponse,
): TransactionDigest {
  if ('transactionDigest' in tx) {
    return tx.transactionDigest;
  }
  const effects = getTransactionEffects(tx)!;
  return effects.transactionDigest;
}

export function getTransactionSignature(tx: CertifiedTransaction): string {
  return tx.txSignature;
}

export function getTransactionAuthorityQuorumSignInfo(
  tx: CertifiedTransaction,
): AuthorityQuorumSignInfo {
  return tx.authSignInfo;
}

export function getTransactionData(
  tx: CertifiedTransaction,
): SuiTransactionData {
  return tx.data;
}

/* ----------------------------- TransactionData ---------------------------- */

export function getTransactionSender(tx: CertifiedTransaction): SuiAddress {
  return tx.data.sender;
}

export function getTransactionGasObject(
  tx: CertifiedTransaction,
): SuiObjectRef {
  return tx.data.gasPayment;
}

export function getTransactionGasPrice(tx: CertifiedTransaction) {
  return tx.data.gasPrice;
}

export function getTransactionGasBudget(tx: CertifiedTransaction): number {
  return tx.data.gasBudget;
}

export function getTransferObjectTransaction(
  data: SuiTransactionKind,
): TransferObject | undefined {
  return 'TransferObject' in data ? data.TransferObject : undefined;
}

export function getPublishTransaction(
  data: SuiTransactionKind,
): SuiMovePackage | undefined {
  return 'Publish' in data ? data.Publish : undefined;
}

export function getMoveCallTransaction(
  data: SuiTransactionKind,
): MoveCall | undefined {
  return 'Call' in data ? data.Call : undefined;
}

export function getTransferSuiTransaction(
  data: SuiTransactionKind,
): SuiTransferSui | undefined {
  return 'TransferSui' in data ? data.TransferSui : undefined;
}

export function getPayTransaction(data: SuiTransactionKind): Pay | undefined {
  return 'Pay' in data ? data.Pay : undefined;
}

export function getPaySuiTransaction(
  data: SuiTransactionKind,
): PaySui | undefined {
  return 'PaySui' in data ? data.PaySui : undefined;
}

export function getPayAllSuiTransaction(
  data: SuiTransactionKind,
): PayAllSui | undefined {
  return 'PayAllSui' in data ? data.PayAllSui : undefined;
}

export function getChangeEpochTransaction(
  data: SuiTransactionKind,
): SuiChangeEpoch | undefined {
  return 'ChangeEpoch' in data ? data.ChangeEpoch : undefined;
}

export function getConsensusCommitPrologueTransaction(
  data: SuiTransactionKind,
): SuiConsensusCommitPrologue | undefined {
  return 'ConsensusCommitPrologue' in data
    ? data.ConsensusCommitPrologue
    : undefined;
}

export function getTransactions(
  data: CertifiedTransaction,
): SuiTransactionKind[] {
  return data.data.transactions;
}

export function getTransferSuiAmount(data: SuiTransactionKind): bigint | null {
  return 'TransferSui' in data && data.TransferSui.amount
    ? BigInt(data.TransferSui.amount)
    : null;
}

export function getTransactionKindName(
  data: SuiTransactionKind,
): TransactionKindName {
  return Object.keys(data)[0] as TransactionKindName;
}

/* ----------------------------- ExecutionStatus ---------------------------- */

export function getExecutionStatusType(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse,
): ExecutionStatusType | undefined {
  return getExecutionStatus(data)?.status;
}

export function getExecutionStatus(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse,
): ExecutionStatus | undefined {
  return getTransactionEffects(data)?.status;
}

export function getExecutionStatusError(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse,
): string | undefined {
  return getExecutionStatus(data)?.error;
}

export function getExecutionStatusGasSummary(
  data:
    | SuiTransactionResponse
    | SuiExecuteTransactionResponse
    | TransactionEffects,
): GasCostSummary | undefined {
  if (is(data, TransactionEffects)) {
    return data.gasUsed;
  }
  return getTransactionEffects(data)?.gasUsed;
}

export function getTotalGasUsed(
  data:
    | SuiTransactionResponse
    | SuiExecuteTransactionResponse
    | TransactionEffects,
): number | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? gasSummary.computationCost +
        gasSummary.storageCost -
        gasSummary.storageRebate
    : undefined;
}

export function getTotalGasUsedUpperBound(
  data:
    | SuiTransactionResponse
    | SuiExecuteTransactionResponse
    | TransactionEffects,
): number | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? gasSummary.computationCost + gasSummary.storageCost
    : undefined;
}

export function getTransactionEffects(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse,
): TransactionEffects | undefined {
  if ('effects' in data) {
    return `effects` in data.effects ? data.effects.effects : data.effects;
  }
  return 'EffectsCert' in data ? data.EffectsCert.effects.effects : undefined;
}

/* ---------------------------- Transaction Effects --------------------------- */

export function getEvents(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse,
): SuiEvent[] | undefined {
  return getTransactionEffects(data)?.events;
}

export function getCreatedObjects(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse,
): OwnedObjectRef[] | undefined {
  return getTransactionEffects(data)?.created;
}

/* --------------------------- TransactionResponse -------------------------- */

export function getTimestampFromTransactionResponse(
  data: SuiExecuteTransactionResponse | SuiTransactionResponse,
): number | undefined {
  return 'timestamp_ms' in data ? data.timestamp_ms ?? undefined : undefined;
}

export function getParsedSplitCoinResponse(
  data: SuiTransactionResponse,
): SuiParsedSplitCoinResponse | undefined {
  const parsed = data.parsed_data;
  return parsed && 'SplitCoin' in parsed ? parsed.SplitCoin : undefined;
}

export function getParsedMergeCoinResponse(
  data: SuiTransactionResponse,
): SuiParsedMergeCoinResponse | undefined {
  const parsed = data.parsed_data;
  return parsed && 'MergeCoin' in parsed ? parsed.MergeCoin : undefined;
}

export function getParsedPublishResponse(
  data: SuiTransactionResponse,
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
  data: SuiTransactionResponse,
): SuiObject | undefined {
  return getParsedMergeCoinResponse(data)?.updatedCoin;
}

/**
 * Get the updated coin after a split.
 * @param data the response for executing a Split coin transaction
 * @returns the updated state of the original coin object used for the split
 */
export function getCoinAfterSplit(
  data: SuiTransactionResponse,
): SuiObject | undefined {
  return getParsedSplitCoinResponse(data)?.updatedCoin;
}

/**
 * Get the newly created coin after a split.
 * @param data the response for executing a Split coin transaction
 * @returns the updated state of the original coin object used for the split
 */
export function getNewlyCreatedCoinsAfterSplit(
  data: SuiTransactionResponse,
): SuiObject[] | undefined {
  return getParsedSplitCoinResponse(data)?.newCoins;
}

/**
 * Get the newly created coin refs after a split.
 */
export function getNewlyCreatedCoinRefsAfterSplit(
  data: SuiTransactionResponse | SuiExecuteTransactionResponse,
): SuiObjectRef[] | undefined {
  if ('EffectsCert' in data) {
    const effects = data.EffectsCert.effects.effects;
    return effects.created?.map((c) => c.reference);
  }
  if ('effects' in data) {
    const effects =
      'effects' in data.effects ? data.effects.effects : data.effects;
    return effects.created?.map((c) => c.reference);
  }
  return undefined;
}
