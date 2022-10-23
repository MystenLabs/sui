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

export type EventType =
    | 'MoveEvent'
    | 'Publish'
    | 'TransferObject'
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
    event: SuiEvent;
};

export type SuiEvents = SuiEventEnvelope[];

export type SubscriptionId = number;

export type SubscriptionEvent = {
    subscription: SubscriptionId;
    result: SuiEventEnvelope;
};

// mirrors the value defined in https://github.com/MystenLabs/sui/blob/e12f8c58ef7ba17205c4caf5ad2c350cbb01656c/crates/sui-json-rpc/src/api.rs#L27
export const EVENT_QUERY_MAX_LIMIT = 100;
export const DEFAULT_START_TIME = 0;
export const DEFAULT_END_TIME = Number.MAX_SAFE_INTEGER;
