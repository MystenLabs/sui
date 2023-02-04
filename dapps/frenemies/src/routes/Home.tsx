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
      () => setTime(getTime(epoch.timestamp, epoch.prevTimestamp)),
      1000
    );
    return () => clearInterval(interval);
  }, [epoch]);

  // Whether there's an assignment for the current round (either first one
  // or requested for the round via "Play Round X" button).
  const hasAssignment = !!scorecard
    && !!epoch
    && scorecard.data.assignment.epoch == epoch.data.epoch;

  // Metadata of the currently assigned validator.
  const validatorMeta = assignedValidator?.fields.metadata.fields;

  return (
    <>
      <Scoreboard />
      <Round />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">{hasAssignment ? formatGoal(goal) : 'Not Assigned'}</Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">
            {validatorMeta && hasAssignment ? (
              <div className="flex items-center gap-2">
                <Logo
                  src={validatorMeta.image_url as string}
                  size="md"
                  label={validatorMeta.name as string}
                  circle
                />
                <div>{validatorMeta.name}</div>
              </div>
            ) : (
              "--"
            )}
          </Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Time Remaining">
            <div className="text-steel-dark font-light">
              {formatTimeRemaining(timer || 0)}
            </div>
          </Stat>
        </Card>
      </div>
      {hasAssignment && <Assignment />}
      <Validators hasAssignment={hasAssignment} />
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
