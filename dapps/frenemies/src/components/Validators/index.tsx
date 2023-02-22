// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { useScorecard } from "../../network/queries/scorecard";
import { formatGoal } from "../../utils/format";
import { Goal } from "../../network/types";
import { Balance } from "./Balance";
import { Table } from "./Table";
import { Card } from "../Card";

export function Validators({ hasAssignment }: { hasAssignment: boolean }) {
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard();

  // At this point there's no way it errors out.
  if (!currentAccount) {
    return null;
  }

  const assignment = scorecard?.data.assignment;

  return (
    <Card variant="white" spacing="lg">
      <div className="flex items-center justify-between mb-10">
        {assignment && hasAssignment ? (
          <h2 className="text-steel-dark font-normal text-2xl">
            Stake SUI to achieve your goal as{" "}
            {assignment.goal === Goal.Enemy ? "an " : "a "}
            <span className="font-bold">{formatGoal(assignment.goal)}</span>.
          </h2>
        ) : (
          <div />
        )}

        <Balance />
      </div>
      <Table />
    </Card>
  );
}
