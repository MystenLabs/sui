// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress, SUI_FRAMEWORK_ADDRESS } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { SUI_SYSTEM_ID } from "../../../network/queries/sui-system";
import { useMyType } from "../../../network/queries/use-raw";
import { ObjectData } from "../../../network/rawObject";
import { Coin, StakedSui, SUI_COIN } from "../../../network/types";

interface Props {
  stake: ObjectData<StakedSui>;
}

/**
 * Arguments required for CancelDelegationRequest transaction.
 */
interface CancelDelegationRequestTx {
  /** Current stake for the Validator */
  stake: ObjectData<StakedSui>;
  /** Coins to get Gas from */
  coins: ObjectData<Coin>[] | null | undefined;
}

/**
 * Request delegation withdrawal.
 * Can only be called if the Delegation and StakedSui objects are present.
 */
export function CancelDelegation({ stake }: Props) {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  const withdrawDelegation = useMutation(
    ["unstake-validator"],
    async ({ stake, coins }: CancelDelegationRequestTx) => {
      if (!coins || coins.length == 0) {
        return null;
      }

      await signAndExecuteTransaction({
        kind: "moveCall",
        data: {
          packageObjectId: SUI_FRAMEWORK_ADDRESS,
          module: "sui_system",
          function: "cancel_delegation_request",
          gasBudget: 10000,
          typeArguments: [],
          gasPayment: normalizeSuiAddress(coins.pop()!.reference.objectId),
          arguments: [
            SUI_SYSTEM_ID,
            normalizeSuiAddress(stake.reference.objectId),
          ],
        },
      });
    }
  );

  const clickHandler = () =>
    withdrawDelegation.mutate({ stake, coins });

  return (
    <button
      disabled={!coins?.length}
      onClick={clickHandler}
      className="absolute right-0 flex py-1 px-4 text-sm leading-none bg-gradient-to-b from-[#D0E8EF] to-[#B9DAE4] opacity-60 hover:opacity-100 uppercase mr-2 rounded-[4px]"
    >
      Cancel
    </button>
  );
}
