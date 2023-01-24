// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import clsx from "clsx";
import { ReactNode } from "react";
import { ObjectData } from "../../network/rawObject";
import { Assignment, StakedSui, Validator } from "../../network/types";
import { formatAddress } from "../../utils/format";
import { Unstake } from "./actions/Unstake";
import { Stake } from "./actions/Stake";
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

  const stakeByValidator: { [key: string]: ObjectData<StakedSui> } =
    stakes.reduce(
      (acc, stake) =>
        Object.assign(acc, {
          [stake.data.validatorAddress]: stake,
        }),
      {}
    );

  console.log(stakeByValidator);

  return (
    <>
      <GridItem className="px-5 py-4">
        <Header>Rank</Header>
        <Header>Validator</Header>
        <Header>Your Sui Stake</Header>
      </GridItem>
      {sorted.map((validator, index) => {
        const address = validator.metadata.suiAddress;
        return (
          <GridItem
            key={address}
            className="px-5 py-2 rounded-xl bg-[#F5FAFA] text-steel-dark items-center"
          >
            <div>{index + 1}</div>
            <div>{formatAddress(address)}</div>
            <div>
              {stakeByValidator[address] ? (
                <Unstake validator={address} stake={stakeByValidator[address]} />
              ) : (
                <Stake validator={address} />
              )}
            </div>
            {address == assignment.validator && (
              <Target goal={assignment.goal} />
            )}
          </GridItem>
        );
      })}
    </>
  );
}
