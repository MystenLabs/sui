// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, SUI_FRAMEWORK_ADDRESS } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { ObjectData } from "../../../network/rawObject";
import { Delegation, StakedSui } from "../../../network/types";
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

  const withdrawDelegation = useMutation(["unstake-validator"], async () => {
    await signAndExecuteTransaction(
      {
        kind: "moveCall",
        data: {
          packageObjectId: SUI_FRAMEWORK_ADDRESS,
          module: "sui_system",
          function: "request_withdraw_delegation",
          gasBudget: Number(GAS_BUDGET),
          typeArguments: [],
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
    <StakeButton onClick={() => withdrawDelegation.mutate()}>
      Unstake
    </StakeButton>
  );
}
