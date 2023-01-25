// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  normalizeSuiAddress,
  SuiAddress,
  SUI_FRAMEWORK_ADDRESS,
} from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { useMyType } from "../../../network/queries/use-raw";
import { ObjectData } from "../../../network/rawObject";
import { Coin, SUI_COIN } from "../../../network/types";

interface Props {
  validator: SuiAddress;
  /** Amount to Delegate */
  amount: string;
}

/**
 * Arguments required for AddDelegation transaction.
 */
interface AddDelegationTx {
  /** Amount of SUI to stake */
  amount: string;
  /** Validator to stake for */
  validator: SuiAddress;
  /** Coins to stake and use as Gas */
  coins?: ObjectData<Coin>[] | null;
}

/**
 * Requests Delegation object for a Validator.
 * Can only be performed if there's no `StakedSui` (hence no `Delegation`) object.
 */
export function AddDelegation({ validator, amount }: Props) {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  const stakeFor = useMutation(
    ["stake-for-validator"],
    async ({ validator, amount, coins }: AddDelegationTx) => {
      if (!coins || coins.length < 2) {
        return null;
      }

      // using the smallest coin as the Gas payment (DESC order, last element popped)
      const [gas, ...restCoins] = [...coins].sort((a, b) =>
        Number(b.data.value - a.data.value)
      );

      await signAndExecuteTransaction({
        kind: "moveCall",
        data: {
          packageObjectId: SUI_FRAMEWORK_ADDRESS,
          module: "sui_system",
          function: "request_add_delegation_mul_coin",
          gasPayment: normalizeSuiAddress(gas.reference.objectId),
          typeArguments: [],
          gasBudget: 10000,
          arguments: [
            SUI_SYSTEM_ID,
            restCoins.map((c) => normalizeSuiAddress(c.reference.objectId)),
            [amount], // Option<u64> // [amt] = Some(amt)
            normalizeSuiAddress(validator),
          ],
        },
      });
    }
  );

  const handleStake = () => {
    stakeFor.mutate({ validator, coins, amount });
  };

  return (
    <button
      // we can only stake if there's at least 2 coins (one gas and one stake)
      disabled={!coins?.length || coins.length < 2}
      onClick={handleStake}
      className="absolute right-0 flex py-1 px-4 text-sm leading-none bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4] opacity-60 hover:opacity-100  uppercase mr-2 rounded-[4px]"
    >
      Stake
    </button>
  );
}
