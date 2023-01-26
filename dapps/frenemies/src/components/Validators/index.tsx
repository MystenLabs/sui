// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useScorecard } from "../../network/queries/scorecard";
import { useSuiSystem } from "../../network/queries/sui-system";
import { useMyType } from "../../network/queries/use-raw";
import { Goal, StakedSui, STAKED_SUI } from "../../network/types";
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

  const validators = system.validators.fields.active_validators;
  const assignment = scorecard.data.assignment;

  return (
    <Card variant="white" spacing="lg">
      <div className="flex items-center justify-between mb-10">
        <h2 className="text-steel-dark font-normal text-2xl">
          Stake SUI to achieve your goal as{" "}
          {assignment.goal === Goal.Enemy ? "an " : "a "}
          <span className="font-bold">{formatGoal(assignment.goal)}</span>.
        </h2>

        <Balance />
      </div>
      <Table validators={validators} stakes={stakes} />
    </Card>
  );
}
