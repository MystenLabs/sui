// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {ObjectOwner, SuiAddress, TransactionDigest} from './common';
import {ObjectId, SequenceNumber} from './objects';
import {SuiJsonValue} from './transactions';

// event types mirror those in "sui-json-rpc-types/lib.rs"
export type MoveEvent = {
  packageId: ObjectId;
  transactionModule: string;
  sender: SuiAddress;
  type: string;
  fields?: { [key: string]: any };
  bcs: string;
};

export type PublishEvent = {
  sender: SuiAddress;
  packageId: ObjectId;
};

export type CoinBalanceChangeEvent = {
  packageId: ObjectId,
  transactionModule: string,
  sender: SuiAddress,
  owner: ObjectOwner,
  changeType: BalanceChangeType,
  coinType: string,
  coinObjectId: ObjectId,
  version: SequenceNumber,
  amount: number,
};

export type TransferObjectEvent = {
  packageId: ObjectId;
  transactionModule: string;
  sender: SuiAddress;
  recipient: ObjectOwner;
  objectType: string,
  objectId: ObjectId;
  version: SequenceNumber;
};

export type MutateObjectEvent = {
  packageId: ObjectId;
  transactionModule: string;
  sender: SuiAddress;
  objectType: string,
  objectId: ObjectId;
  version: SequenceNumber;
};

export type DeleteObjectEvent = {
  packageId: ObjectId;
  transactionModule: string;
  sender: SuiAddress;
  objectId: ObjectId;
  version: SequenceNumber;
};

export type NewObjectEvent = {
  packageId: ObjectId;
  transactionModule: string;
  sender: SuiAddress;
  recipient: ObjectOwner;
  objectType: string,
  objectId: ObjectId;
  version: SequenceNumber;
};

export type SuiEvent =
  | { moveEvent: MoveEvent }
  | { publish: PublishEvent }
  | { coinBalanceChange: CoinBalanceChangeEvent }
  | { transferObject: TransferObjectEvent }
  | { mutateObject: MutateObjectEvent }
  | { deleteObject: DeleteObjectEvent }
  | { newObject: NewObjectEvent }
  | { epochChange: bigint }
  | { checkpoint: bigint };

export type MoveEventField = {
  path: string;
  value: SuiJsonValue;
};

export type EventQuery =
    | "All"
    | { "Transaction": TransactionDigest }
    | { "MoveModule": { package: ObjectId, module: string } }
    | { "MoveEvent": string }
    | { "EventType": EventType }
    | { "Sender": SuiAddress }
    | { "Recipient": ObjectOwner }
    | { "Object": ObjectId }
    | { "TimeRange": { "start_time": number, "end_time": number } };

export type EventId = {
  txSeq: number,
  eventSeq: number,
}

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

export type BalanceChangeType = "Gas" | "Pay" | "Receive"

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

export type SuiEventEnvelope = {
  timestamp: number;
  txDigest: TransactionDigest;
  id: EventId;  // tx_seq_num:event_seq
  event: SuiEvent;
};

export type SuiEvents = SuiEventEnvelope[];

export type SubscriptionId = number;

export type SubscriptionEvent = {
  subscription: SubscriptionId;
  result: SuiEventEnvelope;
};
