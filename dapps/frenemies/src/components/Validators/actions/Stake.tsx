// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress, SUI_FRAMEWORK_ADDRESS } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { useMyType } from "../../../network/queries/use-raw";
import { ObjectData } from "../../../network/rawObject";
import { Coin, SUI_COIN } from "../../../network/types";

interface Props {
  validator: SuiAddress
}

/**
 * TODO: make the Stake button smarter; add TX logic here
 */
export function Stake({ validator }: Props) {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  const stakeFor = useMutation(
    ["stake-for-validator"],
    async ([validator, coins]: [SuiAddress, ObjectData<Coin>[]]) => {
      if (coins.length < 2) {
        return null;
      }
      // using the smallest coin as the Gas payment (DESC order, last element popped)
      const gas = coins.sort((a, b) => Number(b.data.value - a.data.value)).pop()!;

      await signAndExecuteTransaction({
        kind: "moveCall",
        data: {
          packageObjectId: SUI_FRAMEWORK_ADDRESS,
          module: "sui_system",
          typeArguments: [],
          function: "request_add_delegation_mul_coin",
          gasPayment: gas.reference.objectId,
          gasBudget: 10000,
          arguments: [
            SUI_SYSTEM_ID,
            coins.map((c) => c.reference.objectId),
            [], // Option<None> // TODO: specify amount here: [amt] = Some(amt)
            validator
          ]
        }
      })
    }
  );

  return (
    <div className="w-3/4">
      <div className="relative flex items-center">
        <input
          type="text"
          className="block w-full pr-12 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border"
          placeholder="0 SUI"
          disabled={!!coins}
          // defaultValue={stake?.data.staked.toString() || 0}
        />
        <button className="absolute right-0 flex py-1 px-4 text-sm leading-none bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4] opacity-60 hover:opacity-100  uppercase mr-2 rounded-[4px]">
          Stake All
        </button>
      </div>
    </div>
  );
}
