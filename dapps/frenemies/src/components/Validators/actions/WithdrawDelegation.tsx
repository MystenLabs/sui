// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, SUI_FRAMEWORK_ADDRESS } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { useMyType } from "../../../network/queries/use-raw";
import { ObjectData } from "../../../network/rawObject";
import { Coin, Delegation, StakedSui, SUI_COIN } from "../../../network/types";
import { getGas } from "../../../utils/coins";
import provider from "../../../network/provider";
import { StakeButton } from "../../StakeButton";

interface Props {
  stake: ObjectData<StakedSui>;
  delegation: ObjectData<Delegation>;
}

const GAS_BUDGET = 100000n;

/**
 * Request delegation withdrawal.
 * Can only be called if the Delegation and StakedSui objects are present.
 */
export function WithdrawDelegation({ stake, delegation }: Props) {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  const withdrawDelegation = useMutation(["unstake-validator"], async () => {
    if (!coins || !coins.length) {
      return null;
    }

    const gasPrice = await provider.getReferenceGasPrice();
    const gasRequred = GAS_BUDGET * BigInt(gasPrice);
    const { gas } = getGas(coins, gasRequred);

    if (!gas) {
      return null;
    }

    await signAndExecuteTransaction(
      {
        kind: "moveCall",
        data: {
          packageObjectId: SUI_FRAMEWORK_ADDRESS,
          module: "sui_system",
          function: "request_withdraw_delegation",
          gasBudget: Number(GAS_BUDGET),
          typeArguments: [],
          gasPayment: normalizeSuiAddress(gas.reference.objectId),
          arguments: [
            SUI_SYSTEM_ID,
            normalizeSuiAddress(delegation.reference.objectId),
            normalizeSuiAddress(stake.reference.objectId),
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
      disabled={!coins?.length}
      onClick={() => withdrawDelegation.mutate()}
    >
      Unstake
    </StakeButton>
  );
}
