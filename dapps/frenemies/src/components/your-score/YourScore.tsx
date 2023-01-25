// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { config } from "../../config";
import { useLeaderboard } from "../../network/queries/leaderboard";
import { useScorecard } from "../../network/queries/scorecard";
import { useScorecardHistory } from "../../network/queries/scorecard-history";
import { Stat } from "../Stat";
import { Table } from "./Table";

/**
 * Table representing a Leaderboard
 */
export function YourScore() {
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount);
  const { data: leaderboard } = useLeaderboard(config.VITE_LEADERBOARD);
  const { data: history } = useScorecardHistory(scorecard && scorecard.data.id);

  // TODO: Loading and error states:
  if (scorecard == null || currentAccount == null || leaderboard == null) {
    return null;
  }

  const rank = leaderboard.data.topScores.findIndex(
    (score) => score.name == scorecard.data.name
  );

  return (
    <>
      <div className="flex gap-16 mt-3 mb-7">
        <Stat variant="leaderboard" label="Rank">
          {rank == -1 ? "1000+" : rank + 1}
        </Stat>
        <Stat variant="leaderboard" label="Score">
          {scorecard.data.score}
        </Stat>
      </div>
      <Table
        data={history || []}
        leaderboard={leaderboard.data}
        scorecard={scorecard.data}
      />
    </>
  );
}
