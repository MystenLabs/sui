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

/**  Pretty-pring `Goal` enum; turns values into human-readable strings */
export function formatGoal(goal: Goal): string {
    switch (goal) {
        case Goal.Enemy: return 'Enemy';
        case Goal.Friend: return 'Friend';
        case Goal.Neutral: return 'Neutral';
    }
}

/** Pretty-print balance of the currency based on the decimals */
export function formatBalance(num: bigint, decimals: number): string {
    let withPad = num.toString().padStart(18, '0');

      // remove prepend 0s
      let lhs = withPad.slice(0, decimals).split('');
      while (lhs.length > 1) {
        if (lhs[0] == '0') { lhs.shift(); }
        else break;
      }

      // remove trailing 0s
      let rhs = withPad.slice(decimals).split('');
      while (rhs.length > 0) {
          if (rhs.slice(-1)[0] == '0') { rhs.pop() }
          else break;
      }

      return (rhs.length)
        ? lhs.join('') + '.' + rhs.join('')
        : lhs.join('');
}
