// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation } from "@tanstack/react-query";
import {
  LEADERBOARD,
  Leaderboard,
  LEGACY_SCORECARD,
  SCORECARD,
  Scorecard,
} from "../types";
import { useWalletKit } from "@mysten/wallet-kit";
import { config } from "../../config";
import { normalizeSuiAddress } from "@mysten/sui.js";
import { SUI_SYSTEM_ID } from "./sui-system";
import { useMyType, useRawObject } from "./use-raw";

export function useLegacyScorecard() {
  const res = useMyType<Scorecard>(LEGACY_SCORECARD);

  return {
    ...res,
    data: res.data?.at(0),
  };
}

export function useScorecard() {
  const res = useMyType<Scorecard>(SCORECARD);

  return {
    ...res,
    data: res.data?.at(0),
  };
}

const GAS_BUDGET = 100000n;

export function useRefreshScorecard() {
  const { signAndExecuteTransaction } = useWalletKit();
  const { data: scorecard } = useScorecard();
  const { data: leaderboard } = useRawObject<Leaderboard>(
    config.VITE_LEADERBOARD,
    LEADERBOARD
  );

  return useMutation(["refresh-scorecard"], async () => {
    if (!scorecard) throw new Error("Missing scorecard");
    if (!leaderboard) throw new Error("Missing leaderboard");

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
          normalizeSuiAddress(leaderboard.reference.objectId),
        ],
      },
    });
  });
}
