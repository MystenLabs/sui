// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useScorecard } from "../../network/queries/scorecard";
import { useSuiSystem } from "../../network/queries/sui-system";
import { useMyType } from "../../network/queries/use-raw";
import { StakedSui, STAKED_SUI } from "../../network/types";
import { formatGoal } from "../../utils/format";
import { Card } from "../Card";
import { Balance } from "./Balance";
import { Table } from "./Table";

export function Validators() {
  const { currentAccount } = useWalletKit();
  const { data: system } = useSuiSystem();
  const { data: scorecard } = useScorecard(currentAccount);
  const { data: stakes } = useMyType<StakedSui>(STAKED_SUI, currentAccount);

  // At this point there's no way it errors out.
  if (!system || !scorecard || !stakes || !currentAccount) {
    return null;
  }

  const validators = system.data.validators.activeValidators;
  const assignment = scorecard.data.assignment;
  const goal = formatGoal(assignment.goal);

  return (
    <Card variant="white" spacing="lg">
      <div className="flex items-center justify-between mb-10">
        <h2 className="text-steel-dark font-normal text-2xl">
          Stake SUI to achieve your goal as{" "}
          {goal.charAt(0) == "E" ? "an " : "a "}
          <span className="font-bold">{goal}</span>.
        </h2>

        <Balance />
      </div>
      <Table validators={validators} assignment={assignment} stakes={stakes} />
    </Card>
  );
}
