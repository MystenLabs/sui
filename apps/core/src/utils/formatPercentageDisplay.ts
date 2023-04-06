// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// For unavailable %, return '--' else return the APY number
export function formatPercentageDisplay(
    value: number | null,
    nullDisplay = '--'
) {
    return value === null ? nullDisplay : `${value}%`;
}
