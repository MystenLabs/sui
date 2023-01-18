// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from "@mysten/sui.js";
import { formatAddress } from "./../../utils/format";

/**
 * Leaderboard record.
 */
export type Record = {
  round: number;
  // todo: this should probably be a more general enum
  role: "enemy" | "neutral" | "friend";
  validator: SuiAddress;
  objectiveAchieved: boolean;
  score: number;
};

/**
 * A Single row in the Leaderboard table.
 * Tightly coupled with the Leaderboard component.
 */
function Row({ record }: { record: Record }) {
  return (
    <tr>
      <td>{ record.round }</td>
      <td>{ record.role }</td>
      <td>{ formatAddress(record.validator) }</td>
      <td>{ record.objectiveAchieved ? "Achieved" : "Failed" }</td>
      <td>{ record.score > 0 ? ("+" + record.score) : record.score }</td>
    </tr>
  )
}

export default Row;
