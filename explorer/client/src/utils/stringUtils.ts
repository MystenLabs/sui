// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const IPFS_START_STRING = 'https://ipfs.io/ipfs/';

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

export const handleCoinType = (str: string): string =>
    str === '0x2::coin::Coin<0x2::sui::SUI>'
        ? 'SUI'
        : str.match(/^([a-zA-Z0-9:]*)<([a-zA-Z0-9:]*)>$/)?.[2] || str;

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

    separator = separator || '...';

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
    let result: string;

    try {
        // First check Content-Type in header:
        result = await fetch(transformURL(displayString), {
            signal: signal,
        })
            .then(
                (resp) =>
                    resp?.headers
                        ?.get('Content-Type')
                        ?.split('/')
                        .reverse()?.[0]
                        ?.toUpperCase() || 'Image'
            )
            .catch((err) => {
                console.error(err);
                return 'Image';
            });

        // When Content-Type cannot be accessed (e.g. because of CORS), rely on file extension

        if (result === 'Image') {
            const extension =
                displayString?.split('.').reverse()?.[0]?.toUpperCase() || '';
            if (['JPG', 'JPEG', 'PNG'].includes(extension)) {
                result = extension;
            }
        }
    } catch (err) {
        console.error(err);
        result = 'Image';
    }

    return `1 ${result} File`;
}

/* Currently unused but potentially useful:
 *
 * export const isValidHttpUrl = (url: string) => {
 *     try { new URL(url) }
 *         catch (e) { return false }
 *             return /^https?/.test(url);
 *             };
 */
