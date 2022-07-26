/*
"newObject": {
    "packageId": "0x0000000000000000000000000000000000000002",
    "transactionModule": "coin",
    "sender": "0x9b7a3fb24a52703456a16cb0a5b3ebc481ba53a2",
    "recipient": {
        "AddressOwner": "0x9b7a3fb24a52703456a16cb0a5b3ebc481ba53a2"
    },
    "objectId": "0xa3f1a121797295c6ed5669eb2a59d7893899e671"
}
*/

import type { ObjectId, ObjectOwner, SequenceNumber, SuiAddress } from '@mysten/sui.js';

// event types mirror those in "sui-json-rpc-types/lib.rs"
export type MoveEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    type: string;
    fields?: object;        // TODO - better type
    bcs: string;
}

export type PublishEvent = {
    sender: SuiAddress;
    packageId: ObjectId;
}

export type TransferObjectEvent = {
    packageId: ObjectId;
    transactionModule: string;
    sender: SuiAddress;
    recipient: ObjectOwner;
    objectId: ObjectId;
    version: SequenceNumber;
    type: string;   // TODO - better type
}

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

export type SuiEventType =
    { moveEvent: MoveEvent } |
    { publish: PublishEvent } |
    { transferObject: TransferObjectEvent } |
    { deleteObject: DeleteObjectEvent }|
    { newObject: NewObjectEvent } |
    { epochChange: bigint } |
    { checkpoint: bigint }
