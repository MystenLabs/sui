// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress, ObjectOwner } from "./common";
import { ObjectId, SequenceNumber } from "./objects";


// event types mirror those in "sui-json-rpc-types/lib.rs"
export type MoveEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    type: string;
    fields: { [key: string]: any; }; // TODO - better type
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
