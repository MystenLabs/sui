// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Round } from "../components/round/Round";
import { Leaderboard } from "../components/leaderboard/Leaderboard";
import { Card } from "../components/Card";
import { Stat } from "../components/Stat";
import { Validators } from "../components/Validators";

/**
 * The Home page.
 */
export function Home() {
  return (
    <>
      <Leaderboard />
      <Round num={10} />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">Friend</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">ValidatorA</Stat>
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
