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
import { useWalletKit } from "@mysten/wallet-kit";

/**
 * The Home page.
 */
export function Home() {
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount || '');

  // TODO: Render login screen (not registered)
  // TODO: Track wallet connection and make sure user is logged in
  if (!scorecard || !currentAccount) {
    return null;
  }

  const { assignment } = scorecard.data;

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
