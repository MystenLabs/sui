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
} from 'superstruct';
import {
  ObjectId,
  ObjectIdStruct,
  ObjectOwner,
  ObjectOwnerStruct,
  SequenceNumberStruct,
  SuiAddress,
  SuiAddressStruct,
  TransactionDigest,
  TransactionDigestStruct,
} from './shared';
import { SuiJsonValue } from './transactions';

export const BalanceChangeTypeStruct = union([
  literal('Gas'),
  literal('Pay'),
  literal('Receive'),
]);

export type BalanceChangeType = Infer<typeof BalanceChangeTypeStruct>;

// event types mirror those in "sui-json-rpc-types/lib.rs"
export const MoveEventStruct = object({
  packageId: ObjectIdStruct,
  transactionModule: string(),
  sender: SuiAddressStruct,
  type: string(),
  fields: object(),
  bcs: string(),
});

export type MoveEvent = Infer<typeof MoveEventStruct>;

export const PublishEventStruct = object({
  sender: SuiAddressStruct,
  packageId: ObjectIdStruct,
});

export type PublishEvent = Infer<typeof PublishEventStruct>;

export const CoinBalanceChangeEventStruct = object({
  packageId: ObjectIdStruct,
  transactionModule: string(),
  sender: SuiAddressStruct,
  owner: ObjectOwnerStruct,
  changeType: BalanceChangeTypeStruct,
  coinType: string(),
  coinObjectId: ObjectIdStruct,
  version: SequenceNumberStruct,
  amount: number(),
});

export type CoinBalanceChangeEvent = Infer<typeof CoinBalanceChangeEventStruct>;

export const TransferObjectEventStruct = object({
  packageId: ObjectIdStruct,
  transactionModule: string(),
  sender: SuiAddressStruct,
  recipient: ObjectOwnerStruct,
  objectType: string(),
  objectId: ObjectIdStruct,
  version: SequenceNumberStruct,
});

export type TransferObjectEvent = Infer<typeof TransferObjectEventStruct>;

export const MutateObjectEventStruct = object({
  packageId: ObjectIdStruct,
  transactionModule: string(),
  sender: SuiAddressStruct,
  objectType: string(),
  objectId: ObjectIdStruct,
  version: SequenceNumberStruct,
});

export type MutateObjectEvent = Infer<typeof MutateObjectEventStruct>;

export const DeleteObjectEventStruct = object({
  packageId: ObjectIdStruct,
  transactionModule: string(),
  sender: SuiAddressStruct,
  objectId: ObjectIdStruct,
  version: SequenceNumberStruct,
});

export type DeleteObjectEvent = Infer<typeof DeleteObjectEventStruct>;

export const NewObjectEventStruct = object({
  packageId: ObjectIdStruct,
  transactionModule: string(),
  sender: SuiAddressStruct,
  recipient: ObjectOwnerStruct,
  objectType: string(),
  objectId: ObjectIdStruct,
  version: SequenceNumberStruct,
});

export type NewObjectEvent = Infer<typeof NewObjectEventStruct>;

export const SuiEventStruct = union([
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

export type SuiEvent = Infer<typeof SuiEventStruct>;

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

export const EventIdStruct = object({
  txSeq: number(),
  eventSeq: number(),
});

export type EventId = Infer<typeof EventIdStruct>;

export type PaginatedEvents = {
  data: SuiEvents;
  nextCursor: EventId | null;
};

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

export const SuiEventEnvelopeStruct = object({
  timestamp: number(),
  txDigest: TransactionDigestStruct,
  id: EventIdStruct,
  event: SuiEventStruct,
});

export type SuiEventEnvelope = Infer<typeof SuiEventEnvelopeStruct>;

export type SuiEvents = SuiEventEnvelope[];

export const SubscriptionIdStruct = number();

export type SubscriptionId = Infer<typeof SubscriptionIdStruct>;

export const SubscriptionEventStruct = object({
  subscription: SubscriptionIdStruct,
  result: SuiEventEnvelopeStruct,
});

export type SubscriptionEvent = Infer<typeof SubscriptionEventStruct>;
