// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

/* Currently unused but potentially useful:
 *
 * export const isValidHttpUrl = (url: string) => {
 *     try { new URL(url) }
 *         catch (e) { return false }
 *             return /^https?/.test(url);
 *             };
 */
