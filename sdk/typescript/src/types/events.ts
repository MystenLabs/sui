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
} from 'superstruct';
import {
  ObjectId,
  ObjectOwner,
  SuiAddress,
  TransactionDigest,
  SuiJsonValue,
  SequenceNumber,
} from './common';

export const BalanceChangeTypeStruct = union([
  literal('Gas'),
  literal('Pay'),
  literal('Receive'),
]);

export type BalanceChangeType = Infer<typeof BalanceChangeTypeStruct>;

// event types mirror those in "sui-json-rpc-types/lib.rs"
export const MoveEventStruct = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  type: string(),
  fields: object(),
  bcs: string(),
});

export type MoveEvent = Infer<typeof MoveEventStruct>;

export const PublishEventStruct = object({
  sender: SuiAddress,
  packageId: ObjectId,
});

export type PublishEvent = Infer<typeof PublishEventStruct>;

export const CoinBalanceChangeEventStruct = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  owner: ObjectOwner,
  changeType: BalanceChangeTypeStruct,
  coinType: string(),
  coinObjectId: ObjectId,
  version: SequenceNumber,
  amount: number(),
});

export type CoinBalanceChangeEvent = Infer<typeof CoinBalanceChangeEventStruct>;

export const TransferObjectEventStruct = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  recipient: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});

export type TransferObjectEvent = Infer<typeof TransferObjectEventStruct>;

export const MutateObjectEventStruct = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});

export type MutateObjectEvent = Infer<typeof MutateObjectEventStruct>;

export const DeleteObjectEventStruct = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  objectId: ObjectId,
  version: SequenceNumber,
});

export type DeleteObjectEvent = Infer<typeof DeleteObjectEventStruct>;

export const NewObjectEventStruct = object({
  packageId: ObjectId,
  transactionModule: string(),
  sender: SuiAddress,
  recipient: ObjectOwner,
  objectType: string(),
  objectId: ObjectId,
  version: SequenceNumber,
});

export type NewObjectEvent = Infer<typeof NewObjectEventStruct>;

export const SuiEvent = union([
  object({ moveEvent: MoveEventStruct }),
  object({ publish: PublishEventStruct }),
  object({ coinBalanceChange: CoinBalanceChangeEventStruct }),
  object({ transferObject: TransferObjectEventStruct }),
  object({ mutateObject: MutateObjectEventStruct }),
  object({ deleteObject: DeleteObjectEventStruct }),
  object({ newObject: NewObjectEventStruct }),
  object({ epochChange: bigint() }),
  object({ checkpoint: bigint() }),
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
  txSeq: number(),
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
  id: EventId,
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
