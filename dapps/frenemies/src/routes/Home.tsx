// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Leaderboard from "../components/leaderboard/Leaderboard"

export function Home() {
  return (
    <Leaderboard
      rank={333}
      totalScore={420}
      records={[
        {
          round: 10,
          role: "enemy",
          validator: "0x0000000000000000000000000000000000000000",
          objectiveAchieved: true,
          score: 100,
        },
      ]}
    />
  );
}
