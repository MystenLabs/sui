// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress, ObjectOwner, TransactionDigest } from "./common";
import { ObjectId, SequenceNumber } from "./objects";
import { SuiJsonValue } from "./transactions";


// event types mirror those in "sui-json-rpc-types/lib.rs"
export type MoveEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    type: string;
    fields: { [key: string]: any; };
    bcs: string;
};

export type PublishEvent = {
    sender: SuiAddress;
    packageId: ObjectId;
};

export type TransferObjectEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    recipient: ObjectOwner;
    objectId: ObjectId;
    version: SequenceNumber;
    type: string; // TODO - better type
    amount: number | null;
};

export type DeleteObjectEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    objectId: ObjectId;
};

export type NewObjectEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    recipient: ObjectOwner;
    objectId: ObjectId;
};

export type SuiEvent =
    | { moveEvent: MoveEvent }
    | { publish: PublishEvent }
    | { transferObject: TransferObjectEvent }
    | { deleteObject: DeleteObjectEvent }
    | { newObject: NewObjectEvent }
    | { epochChange: bigint }
    | { checkpoint: bigint };

export type MoveEventField = {
    path: string,
    value: SuiJsonValue
}

export type EventType =
    | "MoveEvent"
    | "Publish"
    | "TransferObject"
    | "DeleteObject"
    | "NewObject"
    | "EpochChange"
    | "Checkpoint";

// mirrors sui_json_rpc_types::SuiEventFilter
export type SuiEventFilter =
    | { "Package" : ObjectId }
    | { "Module" : string }
    | { "MoveEventType" : string }
    | { "MoveEventField" : MoveEventField }
    | { "SenderAddress" : SuiAddress }
    | { "EventType" : EventType }
    | { "All" : SuiEventFilter[] }
    | { "Any" : SuiEventFilter[] }
    | { "And" : [SuiEventFilter, SuiEventFilter] }
    | { "Or" : [SuiEventFilter, SuiEventFilter] };

export type SuiEventEnvelope = {
    timestamp:  number,
    txDigest: TransactionDigest,
    event: SuiEvent
}

export type SubscriptionId = number;

export type SubscriptionEvent = { subscription: SubscriptionId, result: SuiEventEnvelope };