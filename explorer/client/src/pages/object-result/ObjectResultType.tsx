// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getMovePackageContent,
    getObjectId,
    getObjectVersion,
    getObjectOwner,
    getObjectFields,
    getObjectPreviousTransactionDigest,
} from '@mysten/sui.js';

import { type AddressOwner } from '../../utils/api/DefaultRpcClient';
import { parseObjectType } from '../../utils/objectUtils';

import type { GetObjectDataResponse, ObjectOwner } from '@mysten/sui.js';

export type DataType = {
    id: string;
    category?: string;
    owner: string | AddressOwner;
    version: string;
    readonly?: string;
    objType: string;
    name?: string;
    ethAddress?: string;
    ethTokenId?: string;
    contract_id?: { bytes: string };
    data: {
        contents: {
            [key: string]: any;
        };
        owner?: { ObjectOwner: [] };
        tx_digest?: string;
    };
    loadState?: string;
};

export function instanceOfDataType(object: any): object is DataType {
    return object && ['id', 'version', 'objType'].every((x) => x in object);
}

/**
 * Translate the SDK response to the existing data format
 * TODO: We should redesign the rendering logic and data model
 * to make this more extensible and customizable for different Move types
 */
export function translate(o: GetObjectDataResponse): DataType {
    switch (o.status) {
        case 'Exists': {
            return {
                id: getObjectId(o),
                version: getObjectVersion(o)!.toString(),
                objType: parseObjectType(o),
                owner: parseOwner(getObjectOwner(o)!),
                data: {
                    contents: getObjectFields(o) ?? getMovePackageContent(o)!,
                    tx_digest: getObjectPreviousTransactionDigest(o),
                },
            };
        }
        case 'NotExists': {
            // TODO: implement this
            throw new Error(
                `Implement me: Object ${getObjectId(o)} does not exist`
            );
        }
        case 'Deleted': {
            // TODO: implement this
            throw new Error(
                `Implement me: Object ${getObjectId(o)} has been deleted`
            );
        }
        default: {
            throw new Error(`Unexpected status ${o.status} for object ${o}`);
        }
    }
}

function parseOwner(owner: ObjectOwner): string {
    let result = '';
    if (typeof owner === 'string') {
        result = owner;
    } else if ('AddressOwner' in owner) {
        result = owner['AddressOwner'];
    } else {
        result = owner['ObjectOwner'];
    }
    return `SingleOwner(k#${result})`;
}
