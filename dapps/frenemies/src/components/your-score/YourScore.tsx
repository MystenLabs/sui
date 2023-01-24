// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { config } from "../../config";
import { useScorecard } from "../../network/queries/scorecard";
import { useScorecardHistory } from "../../network/queries/scorecard-history";
import { Table } from "./Table";
import { Stat } from "../Stat";
import { useRawObject } from "../../network/queries/use-raw";
import { LEADERBOARD, Leaderboard } from "../../network/types";

/**
 * Table representing a Leaderboard
 */
export function YourScore() {
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount);
  const { data: leaderboard } = useRawObject<Leaderboard>(config.VITE_LEADERBOARD, LEADERBOARD);
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
          {rank == -1 ? '--' : (rank + 1)}
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
