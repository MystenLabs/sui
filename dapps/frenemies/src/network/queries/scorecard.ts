// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getRawObjectParsedUnsafe } from "../rawObject";
import { useMutation, useQuery } from "@tanstack/react-query";
import { LEADERBOARD, Leaderboard, SCORECARD, Scorecard } from "../types";
import provider from "../provider";
import { useWalletKit } from "@mysten/wallet-kit";
import { config } from "../../config";
import { normalizeSuiAddress } from "@mysten/sui.js";
import { SUI_SYSTEM_ID } from "./sui-system";
import { useRawObject } from "./use-raw";

/**
 * Get a Scorecard for an account if this account has at least one.
 *
 * We do not guarantee correct behavior if people registered more than once,
 * lookup is done with `Array.prototype.find` for the first occurrence.
 */
export function useScorecard(account?: string | null) {
  return useQuery(
    ["scorecard", account],
    async () => {
      if (!account) {
        return null;
      }

      const objects = await provider.getObjectsOwnedByAddress(account);
      const search = objects.find((v) => v.type.includes(SCORECARD));

      if (!search) {
        return null;
      }

      return getRawObjectParsedUnsafe<Scorecard>(
        provider,
        search.objectId,
        SCORECARD
      );
    },
    {
      enabled: !!account,
      refetchInterval: 60 * 1000,
    }
  );
}

const GAS_BUDGET = 100000n;

export function useRefreshScorecard() {
  const { signAndExecuteTransaction } = useWalletKit();
  const { currentAccount } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount);
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
