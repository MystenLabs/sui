// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/**
 * Contains data formatting functions
 * @module utils/format
 */

import { SuiAddress } from "@mysten/sui.js";
import { Goal } from "../network/types";

/** Formats address as `0xXXXXX...YYYY` */
export function formatAddress(addr: SuiAddress): string {
    return '0x' + addr.slice(0, 4) + '...' + addr.slice(-4);
}

/**  Pretty pring `Goal` enum; turns values into human-readable strings */
export function formatGoal(goal: Goal): string {
    switch (goal) {
        case Goal.Enemy: return 'Enemy';
        case Goal.Friend: return 'Friend';
        case Goal.Neutral: return 'Neutral';
    }
}
