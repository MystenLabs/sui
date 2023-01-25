// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file implements `sui_getRawObject` RPC call to
// speed up data processing and lessen network load by using BCS

import { ObjectOwner, ObjectStatus, Provider, SuiObjectRef } from "@mysten/sui.js";
import { bcs } from "./bcs";

/**
 * Filling in the missing piece in TS SDK.
 */
export type RawObjectResponse = {
    status: ObjectStatus;
    details: {
        reference: SuiObjectRef;
        owner: ObjectOwner;
        data: {
            /* ... some other fields */
            bcs_bytes: string
        },
    }
};

/**
 * Object data fetching result.
 * Contains both the reference to use in txs and the data.
 */
export type ObjectData<T> = {
    reference: SuiObjectRef;
    data: T;
};

/**
 * Wraps the `sui_getRawObject` method.
 */
export function getRawObject(provider: Provider, objectId: string): Promise<RawObjectResponse> {
    return provider.call('sui_getRawObject', [ objectId ]);
}

/**
 * Wrapper for the `getRawObject` which adds bcs deserialization call on the response.
 */
export async function getRawObjectParsedUnsafe<T>(provider: Provider, objectId: string, bcsType: string): Promise<ObjectData<T>> {
    const objectData = await getRawObject(provider, objectId);

    const {
      reference,
      data: { bcs_bytes },
    } = objectData.details;

    return {
      reference,
      data: bcs.de(bcsType, bcs_bytes, "base64"),
    };
}
