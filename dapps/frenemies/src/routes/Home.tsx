// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Round } from "../components/round/Round";
import { Card } from "../components/Card";
import { Stat } from "../components/Stat";
import { Validators } from "../components/Validators";
import { useScorecard } from "../network/queries/scorecard";
import { formatGoal } from "../utils/format";
import { useWalletKit } from "@mysten/wallet-kit";
import { useNavigate } from "react-router-dom";
import { useEffect } from "react";
import { Scoreboard } from "../components/Scoreboard";
import { useEpoch } from "../network/queries/epoch";
import { Goal } from "../network/types";
import { Assignment } from "../components/Assignment";
import { Logo } from "../components/Validators/Logo";
import { Refresh } from "../components/your-score/Refresh";
import { convertToString, useValidators } from "../network/queries/sui-system";
import { TimeRemaining } from "../components/TimeRemaining";

/**
 * The Home page.
 */
export function Home() {
  const navigate = useNavigate();
  const { data: epoch } = useEpoch();
  const { currentAccount } = useWalletKit();
  const { data: scorecard, isSuccess } = useScorecard();
  const { data: validators } = useValidators();

  const { goal, validator } = scorecard?.data.assignment || {
    goal: Goal.Neutral,
    validator: "not_assigned",
  };

  const assignedValidator = (validators || []).find(
    (v) => v.sui_address.replace("0x", "") === validator
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

  // Whether there's an assignment for the current round (either first one
  // or requested for the round via "Play Round X" button).
  const hasAssignment = true;
  !!scorecard && !!epoch && scorecard.data.assignment.epoch == epoch.data.epoch;

  return (
    <>
      <Scoreboard />
      <Round />
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <Card spacing="sm">
          <Stat label="Your Role">
            {hasAssignment ? (
              formatGoal(goal)
            ) : (
              <Refresh fallback="Not assigned" />
            )}
          </Stat>
        </Card>
        <Card spacing="sm">
          <Stat label="Assigned Validator">
            {assignedValidator && hasAssignment ? (
              <div
                className="flex items-center gap-2"
                role="button"
                onClick={() => {
                  document
                    .getElementById(
                      `validator-${assignedValidator.sui_address}`
                    )
                    ?.scrollIntoView({ behavior: "smooth", block: 'center' });
                }}
              >
                <Logo
                  src={convertToString(assignedValidator.image_url)}
                  size="md"
                  label={convertToString(assignedValidator.name) || ""}
                  circle
                />
                <div>{convertToString(assignedValidator.name)}</div>
              </div>
            ) : (
              "--"
            )}
          </Stat>
        </Card>
        <Card spacing="sm">
          <TimeRemaining />
        </Card>
      </div>
      {hasAssignment && <Assignment />}
      <Validators hasAssignment={hasAssignment} />
    </>
  );
}
