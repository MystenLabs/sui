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
  boolean,
  tuple,
  assign,
  nullable,
} from 'superstruct';

import {
  ObjectId,
  ObjectOwner,
  SequenceNumber,
  SuiAddress,
  SuiJsonValue,
  TransactionDigest,
  TransactionEventDigest,
} from './common';
import { SuiEvent } from './events';
import {
  ObjectDigest,
  SuiGasData,
  SuiMovePackage,
  SuiObjectRef,
} from './objects';

export const EpochId = string();

export const SuiChangeEpoch = object({
  epoch: EpochId,
  storage_charge: string(),
  computation_charge: string(),
  storage_rebate: string(),
  epoch_start_timestamp_ms: optional(string()),
});
export type SuiChangeEpoch = Infer<typeof SuiChangeEpoch>;

export const SuiConsensusCommitPrologue = object({
  epoch: EpochId,
  round: string(),
  commit_timestamp_ms: string(),
});
export type SuiConsensusCommitPrologue = Infer<
  typeof SuiConsensusCommitPrologue
>;

export const Genesis = object({
  objects: array(ObjectId),
});
export type Genesis = Infer<typeof Genesis>;

export const SuiArgument = union([
  literal('GasCoin'),
  object({ Input: number() }),
  object({ Result: number() }),
  object({ NestedResult: tuple([number(), number()]) }),
]);
export type SuiArgument = Infer<typeof SuiArgument>;

export const MoveCallSuiTransaction = object({
  arguments: optional(array(SuiArgument)),
  type_arguments: optional(array(string())),
  package: ObjectId,
  module: string(),
  function: string(),
});
export type MoveCallSuiTransaction = Infer<typeof MoveCallSuiTransaction>;

export const SuiTransaction = union([
  object({ MoveCall: MoveCallSuiTransaction }),
  object({ TransferObjects: tuple([array(SuiArgument), SuiArgument]) }),
  object({ SplitCoins: tuple([SuiArgument, array(SuiArgument)]) }),
  object({ MergeCoins: tuple([SuiArgument, array(SuiArgument)]) }),
  object({
    Publish: union([
      // TODO: Remove this after 0.34 is released:
      tuple([SuiMovePackage, array(ObjectId)]),
      array(ObjectId),
    ]),
  }),
  object({
    Upgrade: union([
      // TODO: Remove this after 0.34 is released:
      tuple([SuiMovePackage, array(ObjectId), ObjectId, SuiArgument]),
      tuple([array(ObjectId), ObjectId, SuiArgument]),
    ]),
  }),
  object({ MakeMoveVec: tuple([nullable(string()), array(SuiArgument)]) }),
]);

export const SuiCallArg = union([
  object({
    type: literal('pure'),
    valueType: optional(string()),
    value: SuiJsonValue,
  }),
  object({
    type: literal('object'),
    objectType: literal('immOrOwnedObject'),
    objectId: ObjectId,
    version: SequenceNumber,
    digest: ObjectDigest,
  }),
  object({
    type: literal('object'),
    objectType: literal('sharedObject'),
    objectId: ObjectId,
    initialSharedVersion: SequenceNumber,
    mutable: boolean(),
  }),
]);
export type SuiCallArg = Infer<typeof SuiCallArg>;

export const ProgrammableTransaction = object({
  transactions: array(SuiTransaction),
  inputs: array(SuiCallArg),
});
export type ProgrammableTransaction = Infer<typeof ProgrammableTransaction>;
export type SuiTransaction = Infer<typeof SuiTransaction>;

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
  | 'ChangeEpoch'
  | 'ConsensusCommitPrologue'
  | 'Genesis'
  | 'ProgrammableTransaction';

export const SuiTransactionBlockKind = union([
  assign(SuiChangeEpoch, object({ kind: literal('ChangeEpoch') })),
  assign(
    SuiConsensusCommitPrologue,
    object({
      kind: literal('ConsensusCommitPrologue'),
    }),
  ),
  assign(Genesis, object({ kind: literal('Genesis') })),
  assign(
    ProgrammableTransaction,
    object({ kind: literal('ProgrammableTransaction') }),
  ),
]);
export type SuiTransactionBlockKind = Infer<typeof SuiTransactionBlockKind>;

export const SuiTransactionBlockData = object({
  // Eventually this will become union(literal('v1'), literal('v2'), ...)
  messageVersion: literal('v1'),
  transaction: SuiTransactionBlockKind,
  sender: SuiAddress,
  gasData: SuiGasData, // this shit is diff bw wallet and explorer
});
export type SuiTransactionBlockData = Infer<typeof SuiTransactionBlockData>;

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
  computationCost: string(),
  storageCost: string(),
  storageRebate: string(),
  nonRefundableStorageFee: string(),
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

export const OwnedObjectRef = object({
  owner: ObjectOwner,
  reference: SuiObjectRef,
});
export type OwnedObjectRef = Infer<typeof OwnedObjectRef>;
export const TransactionEffectsModifiedAtVersions = object({
  objectId: ObjectId,
  sequenceNumber: SequenceNumber,
});

export const TransactionEffects = object({
  // Eventually this will become union(literal('v1'), literal('v2'), ...)
  messageVersion: literal('v1'),

  /** The status of the execution */
  status: ExecutionStatus,
  /** The epoch when this transaction was executed */
  executedEpoch: EpochId,
  /** The version that every modified (mutated or deleted) object had before it was modified by this transaction. **/
  modifiedAtVersions: optional(array(TransactionEffectsModifiedAtVersions)),
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

const ReturnValueType = tuple([array(number()), string()]);
const MutableReferenceOutputType = tuple([
  SuiArgument,
  array(number()),
  string(),
]);
const ExecutionResultType = object({
  mutableReferenceOutputs: optional(array(MutableReferenceOutputType)),
  returnValues: optional(array(ReturnValueType)),
});

export const DevInspectResults = object({
  effects: TransactionEffects,
  events: TransactionEvents,
  results: optional(array(ExecutionResultType)),
  error: optional(string()),
});
export type DevInspectResults = Infer<typeof DevInspectResults>;

export type SuiTransactionBlockResponseQuery = {
  filter?: TransactionFilter;
  options?: SuiTransactionBlockResponseOptions;
};

export type TransactionFilter =
  | { Checkpoint: string }
  | { FromAndToAddress: { from: string; to: string } }
  | { TransactionKind: string }
  | {
      MoveFunction: {
        package: ObjectId;
        module: string | null;
        function: string | null;
      };
    }
  | { InputObject: ObjectId }
  | { ChangedObject: ObjectId }
  | { FromAddress: SuiAddress }
  | { ToAddress: SuiAddress };

export type EmptySignInfo = object;
export type AuthorityName = Infer<typeof AuthorityName>;
export const AuthorityName = string();

export const SuiTransactionBlock = object({
  data: SuiTransactionBlockData,
  txSignatures: array(string()),
});
export type SuiTransactionBlock = Infer<typeof SuiTransactionBlock>;

export const SuiObjectChangePublished = object({
  type: literal('published'),
  packageId: ObjectId,
  version: SequenceNumber,
  digest: ObjectDigest,
  modules: array(string()),
});
export type SuiObjectChangePublished = Infer<typeof SuiObjectChangePublished>;

export const SuiObjectChangeTransferred = object({
  type: literal('transferred'),
  sender: SuiAddress,
  recipient: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
  digest: ObjectDigest,
});
export type SuiObjectChangeTransferred = Infer<
  typeof SuiObjectChangeTransferred
>;

export const SuiObjectChangeMutated = object({
  type: literal('mutated'),
  sender: SuiAddress,
  owner: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
  previousVersion: SequenceNumber,
  digest: ObjectDigest,
});
export type SuiObjectChangeMutated = Infer<typeof SuiObjectChangeMutated>;

export const SuiObjectChangeDeleted = object({
  type: literal('deleted'),
  sender: SuiAddress,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});
export type SuiObjectChangeDeleted = Infer<typeof SuiObjectChangeDeleted>;

export const SuiObjectChangeWrapped = object({
  type: literal('wrapped'),
  sender: SuiAddress,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});
export type SuiObjectChangeWrapped = Infer<typeof SuiObjectChangeWrapped>;

export const SuiObjectChangeCreated = object({
  type: literal('created'),
  sender: SuiAddress,
  owner: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
  digest: ObjectDigest,
});
export type SuiObjectChangeCreated = Infer<typeof SuiObjectChangeCreated>;

export const SuiObjectChange = union([
  SuiObjectChangePublished,
  SuiObjectChangeTransferred,
  SuiObjectChangeMutated,
  SuiObjectChangeDeleted,
  SuiObjectChangeWrapped,
  SuiObjectChangeCreated,
]);
export type SuiObjectChange = Infer<typeof SuiObjectChange>;

export const BalanceChange = object({
  owner: ObjectOwner,
  coinType: string(),
  /* Coin balance change(positive means receive, negative means send) */
  amount: string(),
});

export const SuiTransactionBlockResponse = object({
  digest: TransactionDigest,
  transaction: optional(SuiTransactionBlock),
  effects: optional(TransactionEffects),
  events: optional(TransactionEvents),
  timestampMs: optional(string()),
  checkpoint: optional(string()),
  confirmedLocalExecution: optional(boolean()),
  objectChanges: optional(array(SuiObjectChange)),
  balanceChanges: optional(array(BalanceChange)),
  /* Errors that occurred in fetching/serializing the transaction. */
  errors: optional(array(string())),
});
export type SuiTransactionBlockResponse = Infer<
  typeof SuiTransactionBlockResponse
>;

export const SuiTransactionBlockResponseOptions = object({
  /* Whether to show transaction input data. Default to be false. */
  showInput: optional(boolean()),
  /* Whether to show transaction effects. Default to be false. */
  showEffects: optional(boolean()),
  /* Whether to show transaction events. Default to be false. */
  showEvents: optional(boolean()),
  /* Whether to show object changes. Default to be false. */
  showObjectChanges: optional(boolean()),
  /* Whether to show coin balance changes. Default to be false. */
  showBalanceChanges: optional(boolean()),
});

export type SuiTransactionBlockResponseOptions = Infer<
  typeof SuiTransactionBlockResponseOptions
>;

export const PaginatedTransactionResponse = object({
  data: array(SuiTransactionBlockResponse),
  nextCursor: nullable(TransactionDigest),
  hasNextPage: boolean(),
});
export type PaginatedTransactionResponse = Infer<
  typeof PaginatedTransactionResponse
>;
export const DryRunTransactionBlockResponse = object({
  effects: TransactionEffects,
  events: TransactionEvents,
  objectChanges: array(SuiObjectChange),
  balanceChanges: array(BalanceChange),
  // TODO: Remove optional when this is rolled out to all networks:
  input: optional(SuiTransactionBlockData),
});
export type DryRunTransactionBlockResponse = Infer<
  typeof DryRunTransactionBlockResponse
>;

/* -------------------------------------------------------------------------- */
/*                              Helper functions                              */
/* -------------------------------------------------------------------------- */

export function getTransaction(
  tx: SuiTransactionBlockResponse,
): SuiTransactionBlock | undefined {
  return tx.transaction;
}

export function getTransactionDigest(
  tx: SuiTransactionBlockResponse,
): TransactionDigest {
  return tx.digest;
}

export function getTransactionSignature(
  tx: SuiTransactionBlockResponse,
): string[] | undefined {
  return tx.transaction?.txSignatures;
}

/* ----------------------------- TransactionData ---------------------------- */

export function getTransactionSender(
  tx: SuiTransactionBlockResponse,
): SuiAddress | undefined {
  return tx.transaction?.data.sender;
}

export function getGasData(
  tx: SuiTransactionBlockResponse,
): SuiGasData | undefined {
  return tx.transaction?.data.gasData;
}

export function getTransactionGasObject(
  tx: SuiTransactionBlockResponse,
): SuiObjectRef[] | undefined {
  return getGasData(tx)?.payment;
}

export function getTransactionGasPrice(tx: SuiTransactionBlockResponse) {
  return getGasData(tx)?.price;
}

export function getTransactionGasBudget(tx: SuiTransactionBlockResponse) {
  return getGasData(tx)?.budget;
}

export function getChangeEpochTransaction(
  data: SuiTransactionBlockKind,
): SuiChangeEpoch | undefined {
  return data.kind === 'ChangeEpoch' ? data : undefined;
}

export function getConsensusCommitPrologueTransaction(
  data: SuiTransactionBlockKind,
): SuiConsensusCommitPrologue | undefined {
  return data.kind === 'ConsensusCommitPrologue' ? data : undefined;
}

export function getTransactionKind(
  data: SuiTransactionBlockResponse,
): SuiTransactionBlockKind | undefined {
  return data.transaction?.data.transaction;
}

export function getTransactionKindName(
  data: SuiTransactionBlockKind,
): TransactionKindName {
  return data.kind;
}

export function getProgrammableTransaction(
  data: SuiTransactionBlockKind,
): ProgrammableTransaction | undefined {
  return data.kind === 'ProgrammableTransaction' ? data : undefined;
}

/* ----------------------------- ExecutionStatus ---------------------------- */

export function getExecutionStatusType(
  data: SuiTransactionBlockResponse,
): ExecutionStatusType | undefined {
  return getExecutionStatus(data)?.status;
}

export function getExecutionStatus(
  data: SuiTransactionBlockResponse,
): ExecutionStatus | undefined {
  return getTransactionEffects(data)?.status;
}

export function getExecutionStatusError(
  data: SuiTransactionBlockResponse,
): string | undefined {
  return getExecutionStatus(data)?.error;
}

export function getExecutionStatusGasSummary(
  data: SuiTransactionBlockResponse | TransactionEffects,
): GasCostSummary | undefined {
  if (is(data, TransactionEffects)) {
    return data.gasUsed;
  }
  return getTransactionEffects(data)?.gasUsed;
}

export function getTotalGasUsed(
  data: SuiTransactionBlockResponse | TransactionEffects,
): bigint | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? BigInt(gasSummary.computationCost) +
        BigInt(gasSummary.storageCost) -
        BigInt(gasSummary.storageRebate)
    : undefined;
}

export function getTotalGasUsedUpperBound(
  data: SuiTransactionBlockResponse | TransactionEffects,
): bigint | undefined {
  const gasSummary = getExecutionStatusGasSummary(data);
  return gasSummary
    ? BigInt(gasSummary.computationCost) + BigInt(gasSummary.storageCost)
    : undefined;
}

export function getTransactionEffects(
  data: SuiTransactionBlockResponse,
): TransactionEffects | undefined {
  return data.effects;
}

/* ---------------------------- Transaction Effects --------------------------- */

export function getEvents(
  data: SuiTransactionBlockResponse,
): SuiEvent[] | undefined {
  return data.events;
}

export function getCreatedObjects(
  data: SuiTransactionBlockResponse,
): OwnedObjectRef[] | undefined {
  return getTransactionEffects(data)?.created;
}

/* --------------------------- TransactionResponse -------------------------- */

export function getTimestampFromTransactionResponse(
  data: SuiTransactionBlockResponse,
): string | undefined {
  return data.timestampMs ?? undefined;
}

/**
 * Get the newly created coin refs after a split.
 */
export function getNewlyCreatedCoinRefsAfterSplit(
  data: SuiTransactionBlockResponse,
): SuiObjectRef[] | undefined {
  return getTransactionEffects(data)?.created?.map((c) => c.reference);
}

export function getObjectChanges(
  data: SuiTransactionBlockResponse,
): SuiObjectChange[] | undefined {
  return data.objectChanges;
}

export function getPublishedObjectChanges(
  data: SuiTransactionBlockResponse,
): SuiObjectChangePublished[] {
  return (
    (data.objectChanges?.filter((a) =>
      is(a, SuiObjectChangePublished),
    ) as SuiObjectChangePublished[]) ?? []
  );
}
