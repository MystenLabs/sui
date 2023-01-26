// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from "react";
import { Leaderboard, ScorecardUpdatedEvent } from "../../network/types";
import { formatGoal, formatAddress } from "../../utils/format";

interface Props {
  data: ScorecardUpdatedEvent[];
  leaderboard: Leaderboard;
}

const Cell = ({
  as: As = "td",
  children,
}: {
  as?: "th" | "td";
  children: ReactNode;
}) => (
  <As className="text-left text-base font-normal leading-tight py-3">
    {children}
  </As>
);

export function Table({ data, leaderboard }: Props) {
  return (
    <div className="overflow-y-scroll max-h-60">
      <table className="table-fixed w-full">
        <thead>
          <tr>
            <Cell as="th">Round</Cell>
            <Cell as="th">Role</Cell>
            <Cell as="th">Assigned Validator</Cell>
            <Cell as="th">Objective</Cell>
            <Cell as="th">Score</Cell>
          </tr>
        </thead>
        <tbody>
          {data.map((evt) => {
            const { epoch, goal, validator } = evt.assignment;
            const round = epoch - leaderboard.startEpoch;

            return (
              <tr key={epoch.toString()} className="border-t border-white/20">
                <Cell>{round.toString()}</Cell>
                <Cell>{formatGoal(goal)}</Cell>
                <Cell>{formatAddress(validator)}</Cell>
                <Cell>{evt.epochScore !== 0 ? "Achieved" : "Failed"}</Cell>
                <Cell>
                  {evt.epochScore !== 0
                    ? `${evt.totalScore} (+${evt.epochScore})`
                    : `${evt.totalScore}`}
                </Cell>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
