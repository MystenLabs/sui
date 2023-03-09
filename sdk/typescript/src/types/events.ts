// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  object,
  number,
  string,
  bigint,
  union,
  literal,
  Infer,
  array,
  record,
  any,
  optional,
} from 'superstruct';
import {
  ObjectId,
  ObjectOwner,
  SuiAddress,
  TransactionDigest,
  SuiJsonValue,
  SequenceNumber,
} from './common';

export const BalanceChangeType = union([
  literal('Gas'),
  literal('Pay'),
  literal('Receive'),
]);

export type BalanceChangeType = Infer<typeof BalanceChangeType>;

// event types mirror those in "sui-json-rpc-types/lib.rs"
export const MoveEvent = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  type: string(),
  fields: record(string(), any()),
  bcs: string(),
});

export type MoveEvent = Infer<typeof MoveEvent>;

export const PublishEvent = object({
  sender: SuiAddress,
  packageId: ObjectId,
  version: optional(number()),
  digest: optional(string()),
});

export type PublishEvent = Infer<typeof PublishEvent>;

export const CoinBalanceChangeEvent = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  owner: ObjectOwner,
  changeType: BalanceChangeType,
  coinType: string(),
  coinObjectId: ObjectId,
  version: SequenceNumber,
  amount: number(),
});

export type CoinBalanceChangeEvent = Infer<typeof CoinBalanceChangeEvent>;

export const TransferObjectEvent = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  recipient: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});

export type TransferObjectEvent = Infer<typeof TransferObjectEvent>;

export const MutateObjectEvent = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});

export type MutateObjectEvent = Infer<typeof MutateObjectEvent>;

export const DeleteObjectEvent = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  objectId: ObjectId,
  version: SequenceNumber,
});

export type DeleteObjectEvent = Infer<typeof DeleteObjectEvent>;

export const NewObjectEvent = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  recipient: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});

export type NewObjectEvent = Infer<typeof NewObjectEvent>;

// TODO: Figure out if these actually can be bigint:
export const EpochChangeEvent = union([bigint(), number()]);
export type EpochChangeEvent = Infer<typeof EpochChangeEvent>;

export const CheckpointEvent = union([bigint(), number()]);
export type CheckpointEvent = Infer<typeof EpochChangeEvent>;

export const SuiEvent = union([
  object({ moveEvent: MoveEvent }),
  object({ publish: PublishEvent }),
  object({ coinBalanceChange: CoinBalanceChangeEvent }),
  object({ transferObject: TransferObjectEvent }),
  object({ mutateObject: MutateObjectEvent }),
  object({ deleteObject: DeleteObjectEvent }),
  object({ newObject: NewObjectEvent }),
  object({ epochChange: EpochChangeEvent }),
  object({ checkpoint: CheckpointEvent }),
]);
export type SuiEvent = Infer<typeof SuiEvent>;

export type MoveEventField = {
  path: string;
  value: SuiJsonValue;
};

export type EventQuery =
  | 'All'
  | { Transaction: TransactionDigest }
  | { MoveModule: { package: ObjectId; module: string } }
  | { MoveEvent: string }
  | { EventType: EventType }
  | { Sender: SuiAddress }
  | { Recipient: ObjectOwner }
  | { Object: ObjectId }
  | { TimeRange: { start_time: number; end_time: number } };

export const EventId = object({
  txDigest: TransactionDigest,
  eventSeq: number(),
});

export type EventId = Infer<typeof EventId>;

export type EventType =
  | 'MoveEvent'
  | 'Publish'
  | 'TransferObject'
  | 'MutateObject'
  | 'CoinBalanceChange'
  | 'DeleteObject'
  | 'NewObject'
  | 'EpochChange'
  | 'Checkpoint';

// mirrors sui_json_rpc_types::SuiEventFilter
export type SuiEventFilter =
  | { Package: ObjectId }
  | { Module: string }
  | { MoveEventType: string }
  | { MoveEventField: MoveEventField }
  | { SenderAddress: SuiAddress }
  | { EventType: EventType }
  | { All: SuiEventFilter[] }
  | { Any: SuiEventFilter[] }
  | { And: [SuiEventFilter, SuiEventFilter] }
  | { Or: [SuiEventFilter, SuiEventFilter] };

export const SuiEventEnvelope = object({
  timestamp: number(),
  txDigest: TransactionDigest,
  id: EventId, // tx_digest:event_seq
  event: SuiEvent,
});

export type SuiEventEnvelope = Infer<typeof SuiEventEnvelope>;

export type SuiEvents = SuiEventEnvelope[];

export const PaginatedEvents = object({
  data: array(SuiEventEnvelope),
  nextCursor: union([EventId, literal(null)]),
});
export type PaginatedEvents = Infer<typeof PaginatedEvents>;

export const SubscriptionId = number();

export type SubscriptionId = Infer<typeof SubscriptionId>;

export const SubscriptionEvent = object({
  subscription: SubscriptionId,
  result: SuiEventEnvelope,
});

export type SubscriptionEvent = Infer<typeof SubscriptionEvent>;
