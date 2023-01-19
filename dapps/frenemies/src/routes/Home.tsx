// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Round from "../components/round/Round";
import Leaderboard from "../components/leaderboard/Leaderboard"
import Block from "../components/block/Block";
import { useState } from "react";
// import { getScorecard } from "../network/queries/scorecard";
import { getLeaderboard } from "../network/queries/leaderboard";
import type { /* Scorecard, */ Leaderboard as LeaderboardType } from "../network/types";

// Constants for the Application.
// TODO: move them somewhere else.

const LEADERBOARD = import.meta.env.VITE_LEADERBOARD;

/**
 * The Home page.
 */
export function Home() {

  const [leaderboard, setLeaderboard] = useState<LeaderboardType | null>(null);
  // const [scorecard, setScorecard] = useState<Scorecard | null>(null);

  // getScorecard(ACCOUNT).then((res) => (res !== null) && setScorecard(res.data));
  getLeaderboard(LEADERBOARD).then((res) => (res !== null) && setLeaderboard(res.data));

  return (
    <>
      {(leaderboard !== null) && <Leaderboard board={leaderboard} />}
      <Round num={10} />
      <div className="flex content-center my-5">
        <Block title="Your role" value="Friend" />
        <Block title="Assigned Validator" value="ValidatorA" />
        <Block title="Time Remaining" value="10:14:42" />
      </div>
      <div className="card my-10">
        {/* Add current status */}
      </div>
      <div className="card my-10">
        {/* Add staking page (mark the staking goal) */}
      </div>
    </>
  );
}
