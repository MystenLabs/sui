// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectType,
    getMoveObjectType,
    type ObjectOwner,
} from '@mysten/sui.js';

import { findIPFSvalue } from './stringUtils';

import type { GetObjectDataResponse } from '@mysten/sui.js';

export function parseImageURL(data: any): string {
    const url =
        data?.url ||
        // TODO: Remove Legacy format
        data?.display ||
        data?.contents?.display;

    if (!url) return '';

    if (findIPFSvalue(url)) return url;

    // String respresenting true http/https URLs are valid:
    try {
        new URL(url);
        return url;
    } catch {
        return '';
    }
}

export function parseObjectType(data: GetObjectDataResponse): string {
    // TODO: define better naming and typing here
    const dataType = getObjectType(data);
    if (dataType === 'package') {
        return 'Move Package';
    }
    if (dataType === 'moveObject') {
        return getMoveObjectType(data)!;
    }
    return 'unknown';
}

export function getOwnerStr(owner: ObjectOwner | string): string {
    if (typeof owner === 'object') {
        if ('AddressOwner' in owner) return owner.AddressOwner;
        if ('ObjectOwner' in owner) return owner.ObjectOwner;
        if ('Shared' in owner) return 'Shared';
    }
    return owner;
}

export const checkIsPropertyType = (value: any) =>
    ['number', 'string'].includes(typeof value);

export const extractName = (
    contents: Record<string, any> | undefined
): string | undefined => {
    if (!contents || !('name' in contents)) return undefined;
    const name = contents.name;

    if (typeof name === 'string') {
        return name;
    }

    // Dynamic fields
    if (typeof name === 'object' && typeof name?.fields?.name === 'string') {
        return name?.fields?.name;
    }
};
