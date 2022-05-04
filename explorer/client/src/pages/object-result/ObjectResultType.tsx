// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type AddressOwner } from '../../utils/api/DefaultRpcClient';

import type {
    GetObjectInfoResponse,
    MovePackageContent,
    ObjectContent,
    ObjectExistsInfo,
    ObjectNotExistsInfo,
    ObjectOwner,
    ObjectRef,
} from 'sui.js';

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
        tx_digest?: number[] | string;
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
export function translate(o: GetObjectInfoResponse): DataType {
    const { status, details } = o;
    switch (status) {
        case 'Exists': {
            const {
                objectRef: { objectId, version },
                objectType,
                object: { contents, owner, tx_digest },
            } = details as ObjectExistsInfo;

            if (objectType === 'movePackage') {
                return {
                    id: objectId,
                    version: version.toString(),
                    objType: 'Move Package',
                    owner: parseOwner(owner),
                    data: {
                        contents: contents as MovePackageContent,
                        tx_digest,
                    },
                };
            }

            const objectContent = contents as ObjectContent;
            return {
                id: objectId,
                version: version.toString(),
                objType: objectContent['type'],
                owner: parseOwner(owner),
                data: {
                    contents: objectContent.fields,
                    tx_digest,
                },
            };
        }
        case 'NotExists': {
            const { objectId } = details as ObjectNotExistsInfo;
            // TODO: implement this
            throw new Error(`Implement me: Object ${objectId} does not exist`);
        }
        case 'Deleted': {
            const { objectId } = details as ObjectRef;
            // TODO: implement this
            throw new Error(
                `Implement me: Object ${objectId} has been deleted`
            );
        }
        default: {
            throw new Error(`Unexpected status ${status} for object ${o}`);
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
