// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Score } from "../../network/types";

/**
 * A Single row in the Leaderboard table.
 * Tightly coupled with the Leaderboard component.
 */
function Row({ score }: { score: Score }) {
  return (
    <tr>
      <td>{ score.name }</td>
      <td>{ score.score }</td>
      <td>{ score.participation }</td>
    </tr>
  )
}

export default Row;
