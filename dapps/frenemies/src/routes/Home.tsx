// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Round } from "../components/round/Round";
import { Card } from "../components/Card";
import { Stat } from "../components/Stat";
import { Validators } from "../components/Validators";
import { useScorecard } from "../network/queries/scorecard";
import { formatAddress, formatGoal, formatTime } from "../utils/format";
import { useWalletKit } from "@mysten/wallet-kit";
import { useNavigate } from "react-router-dom";
import { useEffect, useState } from "react";
import { Scoreboard } from "../components/Scoreboard";
import { useEpoch } from "../network/queries/epoch";
import { Goal } from "../network/types";
import { config } from "../config";

/**
 * The Home page.
 */
export function Home() {
  const navigate = useNavigate();
  const { data: epoch, isFetched } = useEpoch();
  const { currentAccount } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard(currentAccount);

  const { goal, validator } = scorecard?.data.assignment || {
    goal: Goal.Neutral,
    validator: "not_assigned",
  };

  useEffect(() => {
    if (!currentAccount) {
      navigate("/connect", { replace: true });
    }
  }, [currentAccount]);

  useEffect(() => {
    if (isSuccess && !scorecard) {
      navigate("/connect", { replace: true });
    }
  }, [scorecard, isSuccess]);

  // Should never be triggered ever
  if (!epoch) {
    return null;
  }


  const getTime = () => {
    const timePassed = Date.now() - epoch.timestamp;
    const timeLeft = (+config.VITE_EPOCH_LEN * 60000) - timePassed;
    return timeLeft <= 0 ? 0 : timeLeft;
  }

  // don't kill me for this
  const [timer, setTime] = useState(getTime());

  useEffect(() => {
    const interval = setInterval(() => setTime(getTime()), 1000);
    return () => clearInterval(interval);
  }, [epoch]);

  return (
    <>
      <Scoreboard />
      <Round />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">{formatGoal(goal)}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">{formatAddress(validator)}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Time Remaining">
            <div className="text-steel-dark font-light">{formatTime(timer)}</div>
          </Stat>
        </Card>
      </div>
      <Validators />
    </>
  );
}
