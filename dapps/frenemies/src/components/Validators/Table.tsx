// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from "clsx";
import { ReactNode } from "react";
import { ObjectData } from "../../network/rawObject";
import { Assignment, StakedSui, Validator } from "../../network/types";
import { formatAddress } from "../../utils/format";
import { Stake } from "./Stake";
import { Target } from "./Target";

function Header({ children }: { children: ReactNode }) {
  return (
    <div className="text-left font-normal uppercase text-base text-steel-dark">
      {children}
    </div>
  );
}

interface Props {
  /** Set of 40 currently active validators */
  validators: Validator[];
  /** My assignment */
  assignment: Assignment;
  /** Currently staked Sui */
  stakes: ObjectData<StakedSui>[];
}

function GridItem({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={clsx("grid", className)}
      style={{
        gridTemplateColumns:
          "minmax(100px, 1fr) minmax(100px, 2fr) minmax(min-content, 5fr) minmax(min-content, 2fr)",
      }}
    >
      {children}
    </div>
  );
}

export function Table({ validators, assignment, stakes }: Props) {
  // sort validators by their voting power in DESC order (not by stake - these are different)
  const sorted = validators.sort((a, b) =>
    Number(b.votingPower - a.votingPower)
  );

  const stakeByValidator: { [key: string]: ObjectData<StakedSui> } = stakes.reduce((acc, stake) => Object.assign(acc, {
    [stake.data.validatorAddress]: stake
  }), {});

  return (
    <>
      <GridItem className="px-5 py-4">
        <Header>Rank</Header>
        <Header>Validator</Header>
        <Header>Your Sui Stake</Header>
      </GridItem>
      {sorted.map((validator, index) => {
        return (
          <GridItem
            key={validator.metadata.suiAddress}
            className="px-5 py-2 rounded-xl bg-[#F5FAFA] text-steel-dark items-center"
          >
            <div>{index + 1 + ` (${validator.votingPower})`}</div>
            <div>{formatAddress(validator.metadata.suiAddress)}</div>
            <div>
              <Stake stake={stakeByValidator[validator.metadata.suiAddress] || null} />
            </div>
            {validator.metadata.suiAddress == assignment.validator && (
              <Target goal={assignment.goal} />
            )}
          </GridItem>
        );
      })}
    </>
  );
}
