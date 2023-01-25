// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Round } from "../components/round/Round";
import { Card } from "../components/Card";
import { Stat } from "../components/Stat";
import { Validators } from "../components/Validators";
import { useScorecard } from "../network/queries/scorecard";
import { formatAddress, formatGoal } from "../utils/format";
import { useWalletKit } from "@mysten/wallet-kit";
import { useNavigate } from "react-router-dom";
import { useEffect } from "react";
import { Scoreboard } from "../components/Scoreboard";

/**
 * The Home page.
 */
export function Home() {
  const navigate = useNavigate();
  const { currentAccount } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard(currentAccount);

  useEffect(() => {
    if (!currentAccount) {
      navigate("/connect", { replace: true });
    }
  }, [currentAccount]);

  useEffect(() => {
    if (isSuccess && !scorecard) {
      navigate("/setup", { replace: true });
    }
  }, [scorecard, isSuccess]);

  // TODO: Render login screen (not registered)
  // TODO: Track wallet connection and make sure user is logged in
  if (!scorecard || !currentAccount) {
    return null;
  }

  const { assignment } = scorecard.data;

  return (
    <>
      <Scoreboard />
      <Round />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">{formatGoal(assignment.goal)}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">
            {formatAddress(assignment.validator)}
          </Stat>
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
