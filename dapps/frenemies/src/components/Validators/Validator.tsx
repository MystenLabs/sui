// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ActiveValidator } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import clsx from "clsx";
import { FormEvent, useState } from "react";
import { useScorecard } from "../../network/queries/scorecard";
import { ObjectData } from "../../network/rawObject";
import { Delegation, StakedSui } from "../../network/types";
import { formatAddress, formatBalance } from "../../utils/format";
import { AddDelegation } from "./actions/AddDelegation";
import { CancelDelegation } from "./actions/CancelDelegation";
import { WithdrawDelegation } from "./actions/WithdrawDelegation";
import { GridItem } from "./GridItem";
import { Target } from "./Target";

interface Props {
  index: number;
  validator: ActiveValidator;
  stake: ObjectData<StakedSui>;
  delegation?: ObjectData<Delegation>;
}

const DEC = 9;

export function ValidatorItem({ index, validator, stake, delegation }: Props) {
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount);
  const address = validator.fields.metadata.fields.sui_address;
  const [amount, setAmount] = useState("");

  const onInputAmount = (evt: FormEvent<HTMLInputElement>) => {
    setAmount(evt.currentTarget.value);
  };

  return (
    <GridItem
      key={address}
      className={clsx(
        "px-5 py-2 rounded-xl text-steel-dark items-center",
        delegation || stake
          ? "bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4]"
          : "bg-[#F5FAFA]"
      )}
    >
      <div>{index + 1}</div>
      <div>{formatAddress(address)}</div>
      <div>
        <div className="w-3/4">
          <div className="relative flex items-center">
            <input
              disabled={!!stake}
              type="number"
              // Some arbitrary decent step value:
              step={0.0001}
              min={0}
              onInput={onInputAmount}
              className="block w-full pr-28 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border appearance-none"
              placeholder="0 SUI"
              defaultValue={
                stake &&
                formatBalance(stake?.data.staked.toString() || "0", DEC)
              }
            />

            {delegation ? (
              <WithdrawDelegation delegation={delegation!} stake={stake} />
            ) : stake ? (
              <CancelDelegation stake={stake} />
            ) : (
              <AddDelegation validator={address} amount={amount} />
            )}
          </div>
        </div>
      </div>

      {address == scorecard?.data.assignment.validator && (
        <Target goal={scorecard.data.assignment.goal} />
      )}
    </GridItem>
  );
}
