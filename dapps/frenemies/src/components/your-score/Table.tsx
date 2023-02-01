// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from "react";
import { useSuiSystem } from "../../network/queries/sui-system";
import { Leaderboard, ScorecardUpdatedEvent } from "../../network/types";
import { formatGoal, formatAddress } from "../../utils/format";

interface Props {
  data: ScorecardUpdatedEvent[];
  leaderboard: Leaderboard;
  round: bigint;
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

export function Table({ data, round, leaderboard }: Props) {
  const { data: system } = useSuiSystem();
  const activeValidators = system?.validators.fields.active_validators || [];
  const getValidator = (addr: string) => activeValidators
    .find((v) => v.fields.metadata.fields.sui_address.replace('0x', '') == addr)?.fields;

  const dataByRound: { [key: string]: ScorecardUpdatedEvent } = data
    .reduce((acc, row) => Object.assign(acc, {
      [(row.assignment.epoch - leaderboard.startEpoch).toString()]: row
    }), {});

  const tableData: (ScorecardUpdatedEvent | null)[] = [];
  for (let i = 0n; i < round; i++) {
    tableData.push(dataByRound[i.toString()] || null);
  }

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
          {[...tableData].reverse().map((evt, round, arr) => {
            const currRound = arr.length - round - 1;
            let totalScore = 0;

            if (evt) {
              const { goal, validator } = evt.assignment;
              totalScore = evt.totalScore;
              return (
                <tr key={currRound.toString()} className="border-t border-white/20">
                  <Cell>{currRound.toString()}</Cell>
                  <Cell>{formatGoal(goal)}</Cell>
                  <Cell>{getValidator(validator)?.metadata.fields.name || formatAddress(validator)}</Cell>
                  <Cell>{evt.epochScore !== 0 ? "Achieved" : "Failed"}</Cell>
                  <Cell>
                    {evt.epochScore !== 0
                      ? `${evt.totalScore} (+${evt.epochScore})`
                      : `${evt.totalScore}`}
                  </Cell>
                </tr>
              );
            } else {
              return (
                <tr key={currRound} className="border-t border-white/20">
                  <Cell>{currRound.toString()}</Cell>
                  <Cell>--</Cell>
                  <Cell>--</Cell>
                  <Cell>Skipped</Cell>
                  <Cell>{totalScore}</Cell>
                </tr>
              );
            }
          })}
        </tbody>
      </table>
    </div>
  );
}
