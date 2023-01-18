// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Round from "../components/round/Round";
import Leaderboard from "../components/leaderboard/Leaderboard"
import type { Record } from "../components/leaderboard/Row";
import Block from "../components/block/Block";

/**
 * The Home page.
 */
export function Home() {
  return (
    <>
      <Leaderboard
        rank={333}
        totalScore={420}
        records={getRecords()}
      />
      <Round num={10} />
      <div className="flex content-center">
        <Block title="Your role" value="Friend" />
        <Block title="Assigned Validator" value="ValidatorA" />
        <Block title="Time Remaining" value="10:14:42" />
      </div>
    </>
  );
}

/**
 * Just a preset. To be replaced by a data provider (query).
 */
function getRecords(): Record[] {
  return [
    {
      round: 12,
      role: "enemy",
      validator: "0x0000000000000000000000000000000000000000",
      objectiveAchieved: true,
      score: 100,
    },
    {
      round: 11,
      role: "friend",
      validator: "0x0000000000000000000000000000000000000001",
      objectiveAchieved: false,
      score: 0,
    },
    {
      round: 10,
      role: "neutral",
      validator: "0x0000000000000000000000000000000000000010",
      objectiveAchieved: true,
      score: 200,
    },
  ];
}
