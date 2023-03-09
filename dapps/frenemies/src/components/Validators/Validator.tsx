// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiValidatorSummary } from "@mysten/sui.js";
import clsx from "clsx";
import { FormEvent, useState } from "react";
import { useScorecard } from "../../network/queries/scorecard";
import { ObjectData } from "../../network/rawObject";
import { Delegation, StakedSui } from "../../network/types";
import { formatBalance } from "../../utils/format";
import { AddDelegation } from "./actions/AddDelegation";
import { CancelDelegation } from "./actions/CancelDelegation";
import { WithdrawDelegation } from "./actions/WithdrawDelegation";
import { GridItem } from "./GridItem";
import { Logo } from "./Logo";
import { Target } from "./Target";

interface Props {
  index: number;
  validator: SuiValidatorSummary;
  stake: ObjectData<StakedSui>;
  delegation?: ObjectData<Delegation>;
}

const DEC = 9;

export function ValidatorItem({ index, validator, stake, delegation }: Props) {
  const { data: scorecard } = useScorecard();
  const [amount, setAmount] = useState("");

  const onInputAmount = (evt: FormEvent<HTMLInputElement>) => {
    setAmount(evt.currentTarget.value);
  };

  const totalStake = BigInt(validator.next_epoch_stake);

  return (
    <GridItem
      className={clsx(
        "px-5 py-2 rounded-xl text-steel-dark items-center",
        delegation || stake
          ? "bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4]"
          : "bg-[#F5FAFA]"
      )}
    >
      <div id={`validator-${validator.sui_address}`}>{index + 1}</div>
      <div className="flex items-center gap-2.5">
        <div>
          <Logo
            size="md"
            src={validator.image_url}
            fallback={validator.name}
            label={validator.name}
            circle
          />
        </div>
        <div className="space-y-0.5">
          <div className="text-gray-90 text-body font-semibold">
            {validator.name}
          </div>
          <div>
            <span className="text-gray-90 font-semibold text-body">
              {formatBalance(totalStake, DEC)}
            </span>{" "}
            <span className="text-steel-dark font-medium text-body">
              SUI staked
            </span>
          </div>
        </div>
      </div>
      <div>
        <div className="w-3/4">
          <div className="relative flex items-center">
            {stake ? (
              <div className="block w-full pr-28 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border font-bold">
                {formatBalance(stake?.data.staked.toString() || "0", DEC)} SUI
              </div>
            ) : (
              <input
                type="number"
                // Some arbitrary decent step value:
                step={0.0001}
                min={0}
                onInput={onInputAmount}
                className={clsx(
                  "block w-full pr-28 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border appearance-none"
                )}
                placeholder="0 SUI"
                value={amount}
              />
            )}

            {delegation ? (
              <WithdrawDelegation delegation={delegation!} stake={stake} />
            ) : stake ? (
              <CancelDelegation stake={stake} />
            ) : (
              <AddDelegation
                validator={validator.sui_address}
                amount={amount}
              />
            )}
          </div>
        </div>
      </div>

      {validator.sui_address.replace("0x", "") ===
        scorecard?.data.assignment.validator && (
        <Target goal={scorecard.data.assignment.goal} />
      )}
    </GridItem>
  );
}
