// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  normalizeSuiAddress,
  SuiAddress,
  SUI_FRAMEWORK_ADDRESS,
} from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import BigNumber from "bignumber.js";
import provider from "../../../network/provider";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { useMyType } from "../../../network/queries/use-raw";
import { Coin, SUI_COIN } from "../../../network/types";
import { getGas, useManageCoin } from "../../../utils/coins";
import { StakeButton } from "../../StakeButton";

interface Props {
  validator: SuiAddress;
  /** Amount to Delegate */
  amount: string;
}

const SUI_DECIMALS = 9;
const GAS_BUDGET = 100000n;

function toMist(sui: string) {
  return BigInt(new BigNumber(sui).shiftedBy(SUI_DECIMALS).toString());
}

/**
 * Requests Delegation object for a Validator.
 * Can only be performed if there's no `StakedSui` (hence no `Delegation`) object.
 */
export function AddDelegation({ validator, amount }: Props) {
  const manageCoins = useManageCoin();
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  const stake = useMutation(["stake-for-validator"], async () => {
    if (!coins || !coins.length) {
      throw new Error("No coins found.");
    }

    const mistAmount = toMist(amount);

    const gasPrice = await provider.getReferenceGasPrice();
    const gasRequired = GAS_BUDGET * BigInt(gasPrice);
    const { max } = getGas(coins, gasRequired);

    if (mistAmount > max) {
      throw new Error(
        `Requested amount ${mistAmount} is bigger than max ${max}`
      );
    }

    const stakeCoin = await manageCoins(mistAmount, gasRequired);

    await signAndExecuteTransaction(
      {
        kind: "moveCall",
        data: {
          packageObjectId: SUI_FRAMEWORK_ADDRESS,
          module: "sui_system",
          function: "request_add_delegation_mul_coin",
          typeArguments: [],
          gasBudget: Number(GAS_BUDGET),
          arguments: [
            SUI_SYSTEM_ID,
            [stakeCoin],
            [mistAmount.toString()], // Option<u64> // [amt] = Some(amt)
            normalizeSuiAddress(validator),
          ],
        },
      },
      {
        requestType: "WaitForEffectsCert",
      }
    );
  });

  return (
    <StakeButton
      disabled={!amount || !coins?.length || stake.isLoading}
      onClick={() => stake.mutate()}
    >
      Stake
    </StakeButton>
  );
}
