// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiAddress } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
// import { useMutation } from "@tanstack/react-query";
// import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { ObjectData } from "../../../network/rawObject";
import { StakedSui } from "../../../network/types";

interface Props {
  stake: ObjectData<StakedSui>,
  validator: SuiAddress
}

/**
 * TODO: make the Stake button smarter; add TX logic here
 */
export function Unstake({ stake, validator }: Props) {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();

  // const stakeFor = useMutation(
  //   ["stake-for-validator"],
  //   async (validator: SuiAddress) => {
  //     await signAndExecuteTransaction({
  //       kind: "moveCall",
  //       data: {
  //         packageObjectId: SUI_FRAMEWORK_ADDRESS,
  //         module: "sui_system",
  //         function: "request_add_delegation_mul_coin",
  //         arguments: [
  //           SUI_SYSTEM_ID,
  //           // ,
  //           stake.reference.objectId,
  //         ]
  //         /*
  //           self: &mut SuiSystemState,
  //           delegate_stake: Coin<SUI>,
  //           validator_address: address,
  //           ctx: &mut TxContext,
  //         */
  //       }
  //     })
  //   }
  // );

  return (
    <div className="w-3/4">
      <div className="relative flex items-center">
        <input
          type="text"
          className="block w-full pr-12 bg-white rounded-lg py-2 pl-3 border-steel-darker/30 border"
          placeholder="0 SUI"
          defaultValue={stake?.data.staked.toString() || 0}
        />
        <button className="absolute right-0 flex py-1 px-4 text-sm leading-none bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4] opacity-60 uppercase mr-2 rounded-[4px]">
          Unstake
        </button>
      </div>
    </div>
  );
}
