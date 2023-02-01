// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Round } from "../components/round/Round";
import { Card } from "../components/Card";
import { Stat } from "../components/Stat";
import { Validators } from "../components/Validators";
import { useScorecard } from "../network/queries/scorecard";
import { formatGoal, formatTimeRemaining } from "../utils/format";
import { useWalletKit } from "@mysten/wallet-kit";
import { useNavigate } from "react-router-dom";
import { useEffect, useState } from "react";
import { Scoreboard } from "../components/Scoreboard";
import { useEpoch } from "../network/queries/epoch";
import { Goal } from "../network/types";
import { Assignment } from "../components/Assignment";
import { useSuiSystem } from "../network/queries/sui-system";
import { Logo } from "../components/Validators/Logo";

/**
 * The Home page.
 */
export function Home() {
  const navigate = useNavigate();
  const { data: epoch } = useEpoch();
  const { currentAccount } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard(currentAccount);
  const { data: system } = useSuiSystem();

  const { goal, validator } = scorecard?.data.assignment || {
    goal: Goal.Neutral,
    validator: "not_assigned",
  };

  const assignedValidator = system?.validators.fields.active_validators.find(
    (v) => v.fields.metadata.fields.sui_address.replace("0x", "") === validator
  );

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

  const [timer, setTime] = useState(() => getTime(epoch?.timestamp, epoch?.prevTimestamp));

  useEffect(() => {
    if (!epoch) return;

    const interval = setInterval(
      () => setTime(getTime(epoch?.timestamp, epoch?.prevTimestamp)),
      1000
    );
    return () => clearInterval(interval);
  }, [epoch]);

  if (!epoch) {
    return null;
  }

  return (
    <>
      <Scoreboard />
      <Round />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">{formatGoal(goal)}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">
            {assignedValidator ? (
              <div className="flex items-center gap-2">
                <Logo
                  src={
                    assignedValidator.fields.metadata.fields.image_url as string
                  }
                  size="md"
                  label={
                    assignedValidator.fields.metadata.fields.name as string
                  }
                  circle
                />
                <div>{assignedValidator.fields.metadata.fields.name}</div>
              </div>
            ) : (
              "--"
            )}
          </Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Time Remaining">
            <div className="text-steel-dark font-light">
              {timer && formatTimeRemaining(timer)}
            </div>
          </Stat>
        </Card>
      </div>
      <Assignment />
      <Validators />
    </>
  );
}

/**
 * Calculate time left until the next epoch based on the last two timestamps
 * for epoch changes (`timestamp` and `prevTimestamp`).
 */
function getTime(timestamp?: number, prevTimestamp?: number): number | null {
  if (!timestamp || !prevTimestamp) return null;

  const prevEpochLength = timestamp - prevTimestamp;
  const timePassed = Date.now() - timestamp;
  const timeLeft = prevEpochLength - timePassed;

  return timeLeft <= 0 ? 0 : timeLeft;
};
