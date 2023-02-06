// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MoveActiveValidator } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
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
  validator: MoveActiveValidator;
  stake: ObjectData<StakedSui>;
  delegation?: ObjectData<Delegation>;
}

const DEC = 9;

export function ValidatorItem({ index, validator, stake, delegation }: Props) {
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount);
  const metadata = validator.fields.metadata.fields;
  const [amount, setAmount] = useState("");

  const onInputAmount = (evt: FormEvent<HTMLInputElement>) => {
    setAmount(evt.currentTarget.value);
  };

  const delegatedStake =
    +validator.fields.delegation_staking_pool.fields.sui_balance;
  const selfStake = +validator.fields.stake_amount;
  const totalStake = selfStake + delegatedStake;

  return (
    <GridItem
      className={clsx(
        "px-5 py-2 rounded-xl text-steel-dark items-center",
        delegation || stake
          ? "bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4]"
          : "bg-[#F5FAFA]"
      )}
    >
      <div>{index + 1}</div>
      <div className="flex items-center gap-2.5">
        <div>
          <Logo
            size="md"
            src={metadata.image_url as string}
            fallback={metadata.name as string}
            label={metadata.name as string}
            circle
          />
        </div>
        <div className="space-y-0.5">
          <div className="text-gray-90 text-body font-semibold">
            {metadata.name}
          </div>
          <div className="text-frenemies text-body font-medium">
            {formatBalance(totalStake, DEC)} SUI staked
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
              <AddDelegation validator={metadata.sui_address} amount={amount} />
            )}
          </div>
        </div>
      </div>

      {metadata.sui_address.replace("0x", "") ===
        scorecard?.data.assignment.validator && (
        <Target goal={scorecard.data.assignment.goal} />
      )}
    </GridItem>
  );
}
