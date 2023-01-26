// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export function roundFloat(num: number, precision = 4) {
    return parseFloat(num.toFixed(precision));
}
