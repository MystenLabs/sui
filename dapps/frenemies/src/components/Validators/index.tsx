// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useScorecard } from "../../network/queries/scorecard";
import { useSuiSystem } from "../../network/queries/sui-system";
import { formatGoal } from "../../utils/format";
import { Card } from "../Card";
import { Balance } from "./Balance";
import { Table } from "./Table";

export function Validators() {
  const { data: system } = useSuiSystem();
  const { data: scorecard } = useScorecard('0xcf267442d5331c079fc88f0e4a68c50eb1372426');

  // TODO: SuiSystem doesn't exist.
  // Redundant check; it must exist if RPC is set correctly
  // TODO: What do we do if user is not connected?
  if (!system || !scorecard) {
    return null;
  }

  const validators = system.data.validators.activeValidators;
  const assignment = scorecard.data.assignment;
  const goal = formatGoal(assignment.goal);

  return (
    <Card variant="white" spacing="lg">
      <div className="flex items-center justify-between mb-10">
        <h2 className="text-steel-dark font-normal text-2xl">
          Stake SUI to achieve your goal as {goal.charAt(0) == 'E' ? 'an ' : 'a '}
          <span className="font-bold">{goal}</span>.
        </h2>

        <Balance />
      </div>
      <Table validators={validators} assignment={assignment} />
    </Card>
  );
}
