// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { config } from "../../config";
import provider from "../../network/provider";
import { SUI_SYSTEM_ID, useSuiSystem } from "../../network/queries/sui-system";
import { useMyType } from "../../network/queries/use-raw";
import { ObjectData } from "../../network/rawObject";
import { Coin, Scorecard, SUI_COIN } from "../../network/types";
import { getGas } from "../../utils/coins";

const GAS_BUDGET = 100000n;

interface Props {
  scorecard: ObjectData<Scorecard>;
  leaderboardID: string;
}

export function Refresh({ scorecard, leaderboardID }: Props) {
  const { data: system } = useSuiSystem();
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);

  const refreshScorecard = useMutation(["refresh-scorecard"], async () => {
    if (!coins || !coins.length) {
      return null;
    }

    const gasPrice = await provider.getReferenceGasPrice();
    const gasRequred = GAS_BUDGET * BigInt(gasPrice);
    const { gas } = getGas(coins, gasRequred);

    if (!gas) {
      return null;
    }

    await signAndExecuteTransaction({
      kind: "moveCall",
      data: {
        packageObjectId: config.VITE_PKG,
        module: "frenemies",
        function: "update",
        gasPayment: normalizeSuiAddress(gas.reference.objectId),
        typeArguments: [],
        gasBudget: 1000000,
        arguments: [
          normalizeSuiAddress(scorecard.reference.objectId),
          SUI_SYSTEM_ID,
          normalizeSuiAddress(leaderboardID),
        ],
      },
    }).then(console.log);
  });

  if (!system || scorecard.data.epoch == BigInt(system.epoch) + 1n) {
    return null;
  }

  return (
    <div className="absolute top-0 right-0">
      <button
        className="bg-white shadow-button text-body font-semibold text-frenemies py-3 px-4 rounded-lg inline-flex items-center gap-2"
        onClick={() => {
          refreshScorecard.mutate();
        }}
      >
        <img src="/refresh.svg" alt="refresh" />
        Refresh Score
      </button>
    </div>
  );
}
