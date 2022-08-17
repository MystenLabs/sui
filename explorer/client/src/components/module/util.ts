// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiAddress } from '@mysten/sui.js';

export const SUI_ADDRESS_LENGTH = 20;

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
