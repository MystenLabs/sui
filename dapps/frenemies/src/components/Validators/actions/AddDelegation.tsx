// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  normalizeSuiAddress,
  SuiAddress,
  SUI_FRAMEWORK_ADDRESS,
} from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import provider from "../../../network/provider";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { useMyType } from "../../../network/queries/use-raw";
import { ObjectData } from "../../../network/rawObject";
import { Coin, SUI_COIN } from "../../../network/types";
import { getGas } from "../../../utils/coins";

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

const GAS_BUDGET = 10000n;

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
      if (!coins || !coins.length) {
        return null;
      }

      const gasPrice = await provider.getReferenceGasPrice();
      const gasRequred = GAS_BUDGET * BigInt(gasPrice);
      const { gas, coins: available, max } = getGas(coins, gasRequred);

      if (BigInt(amount) > max) {
        console.log('Requested amt %d is bigger than max %d', amount, max);
        return null;
      }

      if (gas == null) {
        return null;
      }

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
            available.map((c) => normalizeSuiAddress(c.reference.objectId)),
            [max.toString()], // Option<u64> // [amt] = Some(amt)
            normalizeSuiAddress(validator),
          ],
        },
      }).catch(console.log).then(console.log);
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
