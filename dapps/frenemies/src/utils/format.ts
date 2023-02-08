// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from "bignumber.js";
import { Goal } from "../network/types";

const GOAL_TO_COPY = {
  [Goal.Enemy]: "Enemy",
  [Goal.Friend]: "Friend",
  [Goal.Neutral]: "Neutral",
};

export function formatGoal(goal: Goal): string {
  return GOAL_TO_COPY[goal];
}

/** Pretty-print balance of the currency based on the decimals */
export function formatBalance(
  balance: bigint | string | number,
  decimals: number
): string {
  let bn = new BigNumber(balance.toString()).shiftedBy(-1 * decimals);

  if (bn.gt(1)) {
    bn = bn.decimalPlaces(2, BigNumber.ROUND_DOWN);
  }

  return bn.toFormat();
}

/** Print time in the format `hh:mm:ss` */
export function formatTimeRemaining(timer: number): string {
  const date = new Date(timer);
  const hh = date.getUTCHours().toString().padStart(2, "0");
  const mm = date.getUTCMinutes().toString().padStart(2, "0");
  const ss = date.getUTCSeconds().toString().padStart(2, "0");

  // When it's less than a minute, we show a text
  if (hh == "00" && mm == "00") {
    return "About a min";
  }

  return `${hh}:${mm}:${ss}`;
}
