// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useEffect } from "react";
import { useAccount } from "../../network/queries/account";
import { useScorecard } from "../../network/queries/scorecard";
import { Card } from "../Card";
import { Stat } from "../Stat";
import { Table } from "./Table";

/**
 * Leaderboard record.
 */
// export type Record = {
//   round: number;
//   // todo: this should probably be a more general enum
//   role: "enemy" | "neutral" | "friend";
//   validator: SuiAddress;
//   objectiveAchieved: boolean;
//   score: number;
// };

/**
 * Table representing a Leaderboard
 */
export function YourScore() {
  const { data: scorecard } = useScorecard('0xcf267442d5331c079fc88f0e4a68c50eb1372426');

  // TODO: Figure out ways of fetching your txs with a Scorecard
  // NOTE: To do so, fetch events with the scorecard; then parse moveEvents from each
  // of the transactions to get ScorecardUpdated event

  // TODO: Loading and error states:
  if (!scorecard) {
    return null;
  }

  return (
    <Card key="your-score" variant="leaderboard">
      <h2 className="font-semibold text-3xl leading-tight">Your Score</h2>
      <div className="flex gap-16 mt-3 mb-7">
        <Stat variant="leaderboard" label="Rank">
          {scorecard.data.score}
        </Stat>
        <Stat variant="leaderboard" label="Total Score">
          420
        </Stat>
      </div>
      <Table data={scorecard.data} />
    </Card>
  );
}
