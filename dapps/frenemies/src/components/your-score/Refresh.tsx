// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from "@mysten/sui.js";
import { useWalletKit } from "@mysten/wallet-kit";
import { useMutation } from "@tanstack/react-query";
import { config } from "../../config";
import { useEpoch } from "../../network/queries/epoch";
import { SUI_SYSTEM_ID } from "../../network/queries/sui-system";
import { useRawObject } from "../../network/queries/use-raw";
import { ObjectData } from "../../network/rawObject";
import { Leaderboard, LEADERBOARD, Scorecard } from "../../network/types";

const GAS_BUDGET = 100000n;

interface Props {
  scorecard: ObjectData<Scorecard>;
  leaderboardID: string;
  round: bigint;
}

export function Refresh({ scorecard, round, leaderboardID }: Props) {
  const { signAndExecuteTransaction } = useWalletKit();
  const { data: epoch } = useEpoch();
  const { data: leaderboard } = useRawObject<Leaderboard>(
    config.VITE_LEADERBOARD,
    LEADERBOARD
  );

  const refreshScorecard = useMutation(["refresh-scorecard"], async () => {
    await signAndExecuteTransaction({
      kind: "moveCall",
      data: {
        packageObjectId: config.VITE_PKG,
        module: "frenemies",
        function: "update",
        typeArguments: [],
        gasBudget: Number(GAS_BUDGET),
        arguments: [
          normalizeSuiAddress(scorecard.reference.objectId),
          SUI_SYSTEM_ID,
          normalizeSuiAddress(leaderboardID),
        ],
      },
    });
  });

  if (scorecard.data.assignment.epoch == epoch?.data.epoch || !leaderboard) {
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
        Play Round {(round || 0).toString()}
      </button>
    </div>
  );
}
