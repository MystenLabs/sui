// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Number Suffix
export const numberSuffix = (num: number): string => {
    if (num >= 1000000) {
        return (num / 1000000).toFixed(1) + 'M';
    }
    if (num >= 1000) {
        return (num / 1000).toFixed(1) + 'K';
    }
    return num.toString();
};

export const isBigIntOrNumber = (val: any): val is bigint | number =>
    typeof val === 'bigint' || typeof val === 'number';
