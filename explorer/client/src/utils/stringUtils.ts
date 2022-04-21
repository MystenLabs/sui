// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AddressOwner } from './internetapi/DefaultRpcClient';

export function asciiFromNumberBytes(bytes: number[]) {
    return bytes.map((n) => String.fromCharCode(n)).join('');
}

export function hexToAscii(hex: string) {
    if (!hex || typeof hex != 'string') return;
    hex = hex.replace(/^0x/, '');

    var str = '';
    for (var n = 0; n < hex.length; n += 2)
        str += String.fromCharCode(parseInt(hex.substring(n, 2), 16));

    return str;
}

export const trimStdLibPrefix = (str: string): string =>
    str.replace(/^0x2::/, '');

const addrOwnerPattern = /^AddressOwner\(k#(.*)\)$/;
const singleOwnerPattern = /^SingleOwner\(k#(.*)\)$/;
export const extractOwnerData = (owner: string | AddressOwner): string => {
    switch (typeof owner) {
        case 'string':
            const addrExec = addrOwnerPattern.exec(owner);
            if (addrExec !== null) return addrExec[1];

            const result = singleOwnerPattern.exec(owner);
            return result ? result[1] : '';
        case 'object':
            if ('AddressOwner' in owner) {
                let ownerId = extractAddressOwner(owner.AddressOwner);
                return ownerId ? ownerId : '';
            }
            return '';
        default:
            return '';
    }
};

// TODO - this should be removed or updated, now that we don't use number[]
const extractAddressOwner = (addrOwner: number[]): string | null => {
    return asciiFromNumberBytes(addrOwner);
};

export const processDisplayValue = (display: { bytes: number[] } | string) =>
    typeof display === 'object' && 'bytes' in display
        ? asciiFromNumberBytes(display.bytes)
        : display;

export const _toSpace = (str: string) => str.split('_').join(' ');

/* Currently unused but potentially useful:
 *
 * export const isValidHttpUrl = (url: string) => {
 *     try { new URL(url) }
 *         catch (e) { return false }
 *             return /^https?/.test(url);
 *             };
 */
