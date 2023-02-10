// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatAddress } from "@mysten/sui.js";
import { ReactNode } from "react";
import { ROUND_OFFSET } from "../../config";
import { useScorecard } from "../../network/queries/scorecard";
import { useScorecardHistory } from "../../network/queries/scorecard-history";
import {
  convertToString,
  useValidators,
} from "../../network/queries/sui-system";
import { Leaderboard, ScorecardUpdatedEvent } from "../../network/types";
import { formatGoal } from "../../utils/format";
import { Logo } from "../Validators/Logo";

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
  const { data: validators } = useValidators();
  const { data: scorecard } = useScorecard();
  const { isLoading } = useScorecardHistory(scorecard?.data.id);
  const activeValidators = validators || [];
  const getValidator = (addr: string) =>
    activeValidators.find((v) => v.sui_address.replace("0x", "") == addr);

  const dataByRound: { [key: string]: ScorecardUpdatedEvent } = data.reduce(
    (acc, row) =>
      Object.assign(acc, {
        [(row.assignment.epoch - leaderboard.startEpoch).toString()]: row,
      }),
    {}
  );

  const firstRound = Math.min(...Object.keys(dataByRound).map((e) => +e));
  const tableData: (ScorecardUpdatedEvent | null)[] = [];
  for (let i = firstRound; i < round; i++) {
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
            <Cell as="th">Points Scored</Cell>
          </tr>
        </thead>
        <tbody>
          {[...tableData].reverse().map((evt, round, arr) => {
            const currRound = firstRound + arr.length - round - 1;
            if (evt) {
              const { goal, validator } = evt.assignment;
              const validatorMeta = getValidator(validator);
              return (
                <tr
                  key={currRound.toString()}
                  className="border-t border-white/20"
                >
                  <Cell>{currRound.toString()}</Cell>
                  <Cell>{formatGoal(goal)}</Cell>
                  <Cell>
                    <div className="flex items-center gap-2">
                      <Logo
                        src={convertToString(validatorMeta?.image_url)}
                        size="sm"
                        label={convertToString(validatorMeta?.name) || ""}
                        circle
                      />
                      {convertToString(validatorMeta?.name) ||
                        formatAddress(validator)}
                    </div>
                  </Cell>
                  <Cell>{evt.epochScore !== 0 ? "Achieved" : "Failed"}</Cell>
                  <Cell>
                    {(evt.epochScore !== 0 ? "+" : "") + evt.epochScore}
                  </Cell>
                </tr>
              );
            } else {
              return (
                <tr key={currRound} className="border-t border-white/20">
                  <Cell>{currRound.toString()}</Cell>
                  <Cell>--</Cell>
                  <Cell>--</Cell>
                  <Cell>{isLoading ? "--" : "Skipped"}</Cell>
                  <Cell>--</Cell>
                </tr>
              );
            }
          })}
        </tbody>
      </table>
    </div>
  );
}
