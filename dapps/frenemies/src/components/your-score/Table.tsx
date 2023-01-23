// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactNode } from "react";
import { Scorecard } from "../../network/types";

interface Props {
  data: Scorecard;
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

export function Table({ data }: Props) {
  return (
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
        {/* {data.topScores.map((score) => (
          <tr className="border-t border-white/20">
            <Cell>{record.round}</Cell>
            <Cell>{record.role}</Cell>
            <Cell>{formatAddress(record.validator)}</Cell>
            <Cell>{record.objectiveAchieved ? "Achieved" : "Failed"}</Cell>
            <Cell>{record.score > 0 ? "+" + record.score : record.score}</Cell>
          </tr>
        ))} */}
      </tbody>
    </table>
  );
}
