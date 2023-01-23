// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Round } from "../components/round/Round";
import { Leaderboard } from "../components/leaderboard/Leaderboard";
import { Card } from "../components/Card";
import { Stat } from "../components/Stat";
import { Validators } from "../components/Validators";
import { YourScore } from "../components/your-score/YourScore";
import { useScorecard } from "../network/queries/scorecard";
import { formatAddress, formatGoal } from "../utils/format";

/**
 * The Home page.
 */
export function Home() {
  const { data } = useScorecard('0xcf267442d5331c079fc88f0e4a68c50eb1372426');

  // TODO: Render login screen (not registered)
  // TODO: Track wallet connection and make sure user is logged in
  if (data == null) {
    return null;
  }

  const assignment = data.data.assignment;

  return (
    <>
      <Leaderboard />
      <YourScore />
      <Round />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">{formatGoal(assignment.goal)}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">{formatAddress(assignment.validator)}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Time Remaining">
            <div className="text-steel-dark font-light">10:14:42</div>
          </Stat>
        </Card>
      </div>
      <Validators />
    </>
  );
}
