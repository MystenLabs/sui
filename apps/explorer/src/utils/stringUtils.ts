// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiAddress } from '@mysten/sui.js';

const IPFS_START_STRING = 'https://ipfs.io/ipfs/';
const SUI_ADDRESS_LENGTH = 20;

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

export const findIPFSvalue = (url: string): string | undefined =>
    url.match(/^ipfs:\/\/(.*)/)?.[1];

export function transformURL(url: string) {
    const found = findIPFSvalue(url);
    if (!found) {
        return url;
    }
    return `${IPFS_START_STRING}${found}`;
}

export function truncate(fullStr: string, strLen: number, separator?: string) {
    if (fullStr.length <= strLen) return fullStr;

    separator = separator || '\u{2026}';

    const sepLen = separator.length,
        charsToShow = strLen - sepLen,
        frontChars = Math.ceil(charsToShow / 2),
        backChars = Math.floor(charsToShow / 2);

    return (
        fullStr.substr(0, frontChars) +
        separator +
        fullStr.substr(fullStr.length - backChars)
    );
}

export async function extractFileType(
    displayString: string,
    signal: AbortSignal
) {
    // First check Content-Type in header:
    const result = await fetch(transformURL(displayString), {
        signal: signal,
    })
        .then(
            (resp) =>
                resp?.headers?.get('Content-Type')?.split('/').reverse()?.[0]
        )
        .catch((err) => console.error(err));

    // Return the Content-Type if found:
    if (result) {
        return result;
    }
    // When Content-Type cannot be accessed (e.g. because of CORS), rely on file extension
    const extension = displayString?.split('.').reverse()?.[0] || '';
    if (['jpg', 'jpeg', 'png'].includes(extension)) {
        return extension;
    } else {
        return 'Image';
    }
}

export async function genFileTypeMsg(
    displayString: string,
    signal: AbortSignal
) {
    return extractFileType(displayString, signal)
        .then((result) => (result === 'Image' ? result : result.toUpperCase()))
        .then((result) => `1 ${result} File`)
        .catch((err) => {
            console.error(err);
            return `1 Image File`;
        });
}

export const alttextgen = (value: number | string | boolean): string =>
    truncate(String(value), 19);

// TODO: Use version of this function from the SDK when it is exposed.
export function normalizeSuiAddress(
    value: string,
    forceAdd0x: boolean = false
): SuiAddress {
    let address = value.toLowerCase();
    if (!forceAdd0x && address.startsWith('0x')) {
        address = address.slice(2);
    }
    const numMissingZeros =
        (SUI_ADDRESS_LENGTH - getHexByteLength(address)) * 2;
    if (numMissingZeros <= 0) {
        return '0x' + address;
    }
    return '0x' + '0'.repeat(numMissingZeros) + address;
}

function getHexByteLength(value: string): number {
    return /^(0x|0X)/.test(value) ? (value.length - 2) / 2 : value.length / 2;
}

/* Currently unused but potentially useful:
 *
 * export const isValidHttpUrl = (url: string) => {
 *     try { new URL(url) }
 *         catch (e) { return false }
 *             return /^https?/.test(url);
 *             };
 */
