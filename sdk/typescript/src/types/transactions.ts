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
  assign,
  nullable,
} from 'superstruct';
import { SuiEvent } from './events';
import { SuiGasData, SuiMovePackage, SuiObjectRef } from './objects';
import {
  ObjectId,
  ObjectOwner,
  SuiAddress,
  SuiJsonValue,
  TransactionDigest,
  TransactionEventDigest,
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
  storage_rebate: number(),
  epoch_start_timestamp_ms: optional(number()),
});
export type SuiChangeEpoch = Infer<typeof SuiChangeEpoch>;

export const SuiConsensusCommitPrologue = object({
  epoch: number(),
  round: number(),
  commit_timestamp_ms: number(),
});
export type SuiConsensusCommitPrologue = Infer<
  typeof SuiConsensusCommitPrologue
>;

export const Pay = object({
  coins: array(SuiObjectRef),
  recipients: array(SuiAddress),
  amounts: array(string()),
});
export type Pay = Infer<typeof Pay>;

export const PaySui = object({
  coins: array(SuiObjectRef),
  recipients: array(SuiAddress),
  amounts: array(string()),
});
export type PaySui = Infer<typeof PaySui>;

export const PayAllSui = object({
  coins: array(SuiObjectRef),
  recipient: SuiAddress,
});
export type PayAllSui = Infer<typeof PayAllSui>;

export const MoveCall = object({
  package: string(),
  module: string(),
  function: string(),
  typeArguments: optional(array(string())),
  arguments: optional(array(SuiJsonValue)),
});
export type MoveCall = Infer<typeof MoveCall>;

export const Genesis = object({
  objects: array(ObjectId),
});
export type Genesis = Infer<typeof Genesis>;

export const SuiArgument = unknown();

export const SuiCommand = union([
  object({
    MoveCall: object({
      arguments: array(SuiArgument),
      type_arguments: array(string()),
      package: ObjectId,
      module: string(),
      function: string(),
    }),
  }),
  object({ TransferObjects: tuple([array(SuiArgument), SuiArgument]) }),
  object({ SplitCoin: tuple([SuiArgument, SuiAddress]) }),
  object({ MergeCoins: tuple([SuiArgument, array(SuiArgument)]) }),
  object({ Publish: SuiMovePackage }),
  object({ MakeMoveVec: tuple([nullable(string()), array(SuiArgument)]) }),
]);

export const ProgrammableTransaction = object({
  commands: array(),
  inputs: array(SuiJsonValue),
});
export type ProgrammableTransaction = Infer<typeof ProgrammableTransaction>;

/**
 * 1. WaitForEffectsCert: waits for TransactionEffectsCert and then returns to the client.
 *    This mode is a proxy for transaction finality.
 * 2. WaitForLocalExecution: waits for TransactionEffectsCert and makes sure the node
 *    executed the transaction locally before returning to the client. The local execution
 *    makes sure this node is aware of this transaction when the client fires subsequent queries.
 *    However, if the node fails to execute the transaction locally in a timely manner,
 *    a bool type in the response is set to false to indicate the case.
 */
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
  | 'Genesis'
  | 'ProgrammableTransaction';

export const SuiTransactionKind = union([
  assign(TransferObject, object({ kind: literal('TransferObject') })),
  assign(SuiMovePackage, object({ kind: literal('Publish') })),
  assign(MoveCall, object({ kind: literal('Call') })),
  assign(SuiTransferSui, object({ kind: literal('TransferSui') })),
  assign(SuiChangeEpoch, object({ kind: literal('ChangeEpoch') })),
  assign(
    SuiConsensusCommitPrologue,
    object({
      kind: literal('ConsensusCommitPrologue'),
    }),
  ),
  assign(Pay, object({ kind: literal('Pay') })),
  assign(PaySui, object({ kind: literal('PaySui') })),
  assign(PayAllSui, object({ kind: literal('PayAllSui') })),
  assign(Genesis, object({ kind: literal('Genesis') })),
  assign(
    ProgrammableTransaction,
    object({ kind: literal('ProgrammableTransaction') }),
  ),
]);
export type SuiTransactionKind = Infer<typeof SuiTransactionKind>;

export const SuiTransactionData = object({
  // Eventually this will become union(literal('v1'), literal('v2'), ...)
  messageVersion: literal('v1'),
  transactions: array(SuiTransactionKind),
  sender: SuiAddress,
  gasData: SuiGasData,
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
  // Eventually this will become union(literal('v1'), literal('v2'), ...)
  messageVersion: literal('v1'),

  /** The status of the execution */
  status: ExecutionStatus,
  /** The epoch when this transaction was executed */
  executedEpoch: EpochId,
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
  /** Object Refs of objects now deleted (the old refs) */
  unwrapped_then_deleted: optional(array(SuiObjectRef)),
  /** Object refs of objects now wrapped in other objects */
  wrapped: optional(array(SuiObjectRef)),
  /**
   * The updated gas object reference. Have a dedicated field for convenient access.
   * It's also included in mutated.
   */
  gasObject: OwnedObjectRef,
  /** The events emitted during execution. Note that only successful transactions emit events */
  eventsDigest: optional(TransactionEventDigest),
  /** The set of transaction digests this transaction depends on */
  dependencies: optional(array(TransactionDigest)),
});
export type TransactionEffects = Infer<typeof TransactionEffects>;

export const TransactionEvents = array(SuiEvent);
export type TransactionEvents = Infer<typeof TransactionEvents>;

export const DryRunTransactionResponse = object({
  effects: TransactionEffects,
  events: TransactionEvents,
});
export type DryRunTransactionResponse = Infer<typeof DryRunTransactionResponse>;

const ReturnValueType = tuple([array(number()), string()]);
const MutableReferenceOutputType = tuple([number(), array(number()), string()]);
const ExecutionResultType = object({
  mutableReferenceOutputs: optional(array(MutableReferenceOutputType)),
  returnValues: optional(array(ReturnValueType)),
});

const DevInspectResultsType = union([
  object({ Ok: array(ExecutionResultType) }),
  object({ Err: string() }),
]);

export const DevInspectResults = object({
  effects: TransactionEffects,
  events: TransactionEvents,
  results: DevInspectResultsType,
});
export type DevInspectResults = Infer<typeof DevInspectResults>;

export type GatewayTxSeqNumber = number;

export const GetTxnDigestsResponse = array(TransactionDigest);
export type GetTxnDigestsResponse = Infer<typeof GetTxnDigestsResponse>;

export const PaginatedTransactionDigests = object({
  data: array(TransactionDigest),
  nextCursor: union([TransactionDigest, literal(null)]),
  hasNextPage: boolean(),
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

export const SuiTransaction = object({
  data: SuiTransactionData,
  txSignatures: array(string()),
});
export type SuiTransaction = Infer<typeof SuiTransaction>;

export const SuiTransactionResponse = object({
  digest: TransactionDigest,
  transaction: optional(SuiTransaction),
  effects: optional(TransactionEffects),
  events: optional(TransactionEvents),
  timestampMs: optional(number()),
  checkpoint: optional(number()),
  confirmedLocalExecution: optional(boolean()),
  /* Errors that occurred in fetching/serializing the transaction. */
  errors: optional(array(string())),
});
export type SuiTransactionResponse = Infer<typeof SuiTransactionResponse>;

export const SuiTransactionResponseOptions = object({
  /* Whether to show transaction input data. Default to be false. */
  showInput: optional(boolean()),
  /* Whether to show transaction effects. Default to be false. */
  showEffects: optional(boolean()),
  /* Whether to show transaction events. Default to be false. */
  showEvents: optional(boolean()),
});

export type SuiTransactionResponseOptions = Infer<
  typeof SuiTransactionResponseOptions
>;

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

export function getTransaction(
  tx: SuiTransactionResponse,
): SuiTransaction | undefined {
  return tx.transaction;
}

export function getTransactionDigest(
  tx: SuiTransactionResponse,
): TransactionDigest {
  return tx.digest;
}

export function getTransactionSignature(
  tx: SuiTransactionResponse,
): string[] | undefined {
  return tx.transaction?.txSignatures;
}

/* ----------------------------- TransactionData ---------------------------- */

export function getTransactionSender(
  tx: SuiTransactionResponse,
): SuiAddress | undefined {
  return tx.transaction?.data.sender;
}

export function getGasData(tx: SuiTransactionResponse): SuiGasData | undefined {
  return tx.transaction?.data.gasData;
}

export function getTransactionGasObject(
  tx: SuiTransactionResponse,
): SuiObjectRef[] | undefined {
  return getGasData(tx)?.payment;
}

export function getTransactionGasPrice(tx: SuiTransactionResponse) {
  return getGasData(tx)?.price;
}

export function getTransactionGasBudget(tx: SuiTransactionResponse) {
  return getGasData(tx)?.budget;
}

export function getTransferObjectTransaction(
  data: SuiTransactionKind,
): TransferObject | undefined {
  return data.kind === 'TransferObject' ? data : undefined;
}

export function getPublishTransaction(
  data: SuiTransactionKind,
): SuiMovePackage | undefined {
  return data.kind === 'Publish' ? data : undefined;
}

export function getMoveCallTransaction(
  data: SuiTransactionKind,
): MoveCall | undefined {
  return data.kind === 'Call' ? data : undefined;
}

export function getTransferSuiTransaction(
  data: SuiTransactionKind,
): SuiTransferSui | undefined {
  return data.kind === 'TransferSui' ? data : undefined;
}

export function getPayTransaction(data: SuiTransactionKind): Pay | undefined {
  return data.kind === 'Pay' ? data : undefined;
}

export function getPaySuiTransaction(
  data: SuiTransactionKind,
): PaySui | undefined {
  return data.kind === 'PaySui' ? data : undefined;
}

export function getPayAllSuiTransaction(
  data: SuiTransactionKind,
): PayAllSui | undefined {
  return data.kind === 'PayAllSui' ? data : undefined;
}

export function getChangeEpochTransaction(
  data: SuiTransactionKind,
): SuiChangeEpoch | undefined {
  return data.kind === 'ChangeEpoch' ? data : undefined;
}

export function getConsensusCommitPrologueTransaction(
  data: SuiTransactionKind,
): SuiConsensusCommitPrologue | undefined {
  return data.kind === 'ConsensusCommitPrologue' ? data : undefined;
}

export function getTransactionKinds(
  data: SuiTransactionResponse,
): SuiTransactionKind[] | undefined {
  return data.transaction?.data.transactions;
}

export function getTransferSuiAmount(data: SuiTransactionKind): bigint | null {
  return data.kind === 'TransferSui' && data.amount
    ? BigInt(data.amount)
    : null;
}

export function getTransactionKindName(
  data: SuiTransactionKind,
): TransactionKindName {
  return data.kind;
}

export function getProgrammableTransaction(
  data: SuiTransactionKind,
): ProgrammableTransaction | undefined {
  return data.kind === 'ProgrammableTransaction' ? data : undefined;
}

/* ----------------------------- ExecutionStatus ---------------------------- */

export function getExecutionStatusType(
  data: SuiTransactionResponse,
): ExecutionStatusType | undefined {
  return getExecutionStatus(data)?.status;
}

export function getExecutionStatus(
  data: SuiTransactionResponse,
): ExecutionStatus | undefined {
  return getTransactionEffects(data)?.status;
}

export function getExecutionStatusError(
  data: SuiTransactionResponse,
): string | undefined {
  return getExecutionStatus(data)?.error;
}

export function getExecutionStatusGasSummary(
  data: SuiTransactionResponse | TransactionEffects,
): GasCostSummary | undefined {
  if (is(data, TransactionEffects)) {
    return data.gasUsed;
  }
  return getTransactionEffects(data)?.gasUsed;
}

export function getTotalGasUsed(
  data: SuiTransactionResponse | TransactionEffects,
): number | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? gasSummary.computationCost +
        gasSummary.storageCost -
        gasSummary.storageRebate
    : undefined;
}

export function getTotalGasUsedUpperBound(
  data: SuiTransactionResponse | TransactionEffects,
): number | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? gasSummary.computationCost + gasSummary.storageCost
    : undefined;
}

export function getTransactionEffects(
  data: SuiTransactionResponse,
): TransactionEffects | undefined {
  return data.effects;
}

/* ---------------------------- Transaction Effects --------------------------- */

export function getEvents(
  data: SuiTransactionResponse,
): SuiEvent[] | undefined {
  return data.events;
}

export function getCreatedObjects(
  data: SuiTransactionResponse,
): OwnedObjectRef[] | undefined {
  return getTransactionEffects(data)?.created;
}

/* --------------------------- TransactionResponse -------------------------- */

export function getTimestampFromTransactionResponse(
  data: SuiTransactionResponse,
): number | undefined {
  return data.timestampMs ?? undefined;
}

/**
 * Get the newly created coin refs after a split.
 */
export function getNewlyCreatedCoinRefsAfterSplit(
  data: SuiTransactionResponse,
): SuiObjectRef[] | undefined {
  return getTransactionEffects(data)?.created?.map((c) => c.reference);
}
