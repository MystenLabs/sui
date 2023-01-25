// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { FormEvent, ReactNode, useState } from "react";
import { ObjectData } from "../../network/rawObject";
import {
  Assignment,
  DELEGATION,
  Delegation,
  StakedSui,
  Validator,
} from "../../network/types";
import { formatAddress, formatBalance } from "../../utils/format";
import { WithdrawDelegation } from "./actions/WithdrawDelegation";
import { AddDelegation } from "./actions/AddDelegation";
import { Target } from "./Target";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMyType } from "../../network/queries/use-raw";
import { CancelDelegation } from "./actions/CancelDelegation";
import { GridItem } from "./GridItem";

/** Number of decimals for SUI */
const DEC = 9;

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

export function Table({ validators, assignment, stakes }: Props) {
  const { currentAccount } = useWalletKit();
  const { data: delegations } = useMyType<Delegation>(
    DELEGATION,
    currentAccount
  );

  // sort validators by their voting power in DESC order (not by stake - these are different)
  const sorted = [...validators].sort((a, b) =>
    Number(b.votingPower - a.votingPower)
  );

  const stakeByValidator: Record<string, ObjectData<StakedSui>> = stakes.reduce(
    (acc, stake) =>
      Object.assign(acc, {
        [stake.data.validatorAddress]: stake,
      }),
    {}
  );

  return (
    <>
      <GridItem className="px-5 py-4">
        <Header>Rank</Header>
        <Header>Validator</Header>
        <Header>Your Sui Stake</Header>
      </GridItem>

      <div className="flex flex-col gap-1">
        {sorted.map((validator, index) => {
          const address = validator.metadata.suiAddress;
          const stake = stakeByValidator[address];
          const delegation =
            stake &&
            (delegations || []).find(
              (d) => d.data.stakedSuiId == stake.data.id
            );

          const [amount, setAmount] = useState("0");
          const onInputAmount = (evt: FormEvent<HTMLInputElement>) => {
            setAmount(fromUserInput(evt.currentTarget.value));
          };

          return (
            <GridItem
              key={address}
              className="px-5 py-2 rounded-xl bg-[#F5FAFA] text-steel-dark items-center"
            >
              <div>{index + 1}</div>
              <div>{formatAddress(address)}</div>
              <div>
                <div className="w-3/4">
                  <div className="relative flex items-center">
                    <input
                      disabled={!!stake}
                      type="text"
                      onInput={onInputAmount}
                      className="block w-full pr-12 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border"
                      placeholder="0 SUI"
                      defaultValue={
                        stake &&
                        formatBalance(stake?.data.staked.toString() || "0", DEC)
                      }
                    />

                    {delegation ? (
                      <WithdrawDelegation
                        delegation={delegation!}
                        stake={stakeByValidator[address]}
                      />
                    ) : stake ? (
                      <CancelDelegation stake={stake} />
                    ) : (
                      <AddDelegation validator={address} amount={amount} />
                    )}
                  </div>
                </div>
              </div>
              {address == assignment.validator && (
                <Target goal={assignment.goal} />
              )}
            </GridItem>
          );
        })}
      </div>
    </>
  );
}

/**
 * Helper function to parse user input (with decimals
 * separator) and turn it into a correctly-formed 9 decimals
 * SUI value.
 */
function fromUserInput(num: string): string {
  // todo: possibly move it as arg
  const decimals = 9;

  if (num.includes(".")) {
    let [lhs, rhs] = num.split(".");
    return lhs + rhs.padEnd(decimals, "0");
  } else {
    return num.padEnd(decimals + num.length, "0");
  }
}
