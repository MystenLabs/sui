// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const defaultPrecision = 2;
export function roundFloat(num: number, precision = defaultPrecision) {
    return parseFloat(num.toFixed(precision));
}
