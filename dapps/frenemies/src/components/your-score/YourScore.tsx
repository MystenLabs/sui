// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletKit } from "@mysten/wallet-kit";
import { config } from "../../config";
import { useScorecard } from "../../network/queries/scorecard";
import { useScorecardHistory } from "../../network/queries/scorecard-history";
import { Table } from "./Table";
import { Stat } from "../Stat";
import { useMyType, useRawObject } from "../../network/queries/use-raw";
import {
  Coin,
  LEADERBOARD,
  Leaderboard,
  Scorecard,
  SUI_COIN,
} from "../../network/types";
import { useMutation } from "@tanstack/react-query";
import { ObjectData } from "../../network/rawObject";
import { normalizeSuiAddress } from "@mysten/sui.js";
import { SUI_SYSTEM_ID } from "../../network/queries/sui-system";
import provider from "../../network/provider";
import { getGas } from "../../utils/coins";

interface RefreshScorecardTx {
  /** Validator to stake for */
  scorecard: ObjectData<Scorecard>;
  /** Leaderboard object data */
  leaderboard: ObjectData<Leaderboard>;
  /** Coins to stake and use as Gas */
  coins?: ObjectData<Coin>[] | null;
}

/** Gas budget for the RefreshScorecardTx */
const GAS_BUDGET = 10000n;

/**
 * Table representing a Leaderboard
 */
export function YourScore() {
  const { currentAccount, signAndExecuteTransaction } = useWalletKit();
  const { data: scorecard } = useScorecard(currentAccount);
  const { data: coins } = useMyType<Coin>(SUI_COIN, currentAccount);
  const { data: history } = useScorecardHistory(scorecard && scorecard.data.id);
  const { data: leaderboard } = useRawObject<Leaderboard>(
    config.VITE_LEADERBOARD,
    LEADERBOARD
  );

  const refreshScorecard = useMutation(
    ["refresh-scorecard"],
    async ({ scorecard, leaderboard, coins }: RefreshScorecardTx) => {
      if (!coins || !coins.length) {
        return null;
      }

      const gasPrice = await provider.getReferenceGasPrice();
      const gasRequred = GAS_BUDGET * BigInt(gasPrice);

      const { gas } = getGas(coins, gasRequred);

      if (gas == null) {
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
            normalizeSuiAddress(leaderboard.reference.objectId),
          ],
        },
      });
    }
  );

  // TODO: Loading and error states:
  if (scorecard == null || currentAccount == null || leaderboard == null) {
    return null;
  }

  const rank = leaderboard
    && leaderboard.data.topScores.findIndex((score) => score.name == scorecard.data.name)
    || -1;

  const refreshHandler = () =>
    refreshScorecard.mutate({
      scorecard,
      leaderboard,
      coins,
    });

  return (
    <>
      <div className="flex gap-16 mt-3 mb-7">
        <Stat variant="leaderboard" label="Rank">
          {rank == -1 ? "--" : rank + 1}
        </Stat>
        <Stat variant="leaderboard" label="Score">
          {scorecard.data.score}
        </Stat>
        <Stat variant="leaderboard" label="Refresh">
          <button onClick={refreshHandler}>Refresh</button>
        </Stat>
      </div>
      <Table data={history || []} leaderboard={leaderboard.data} />
    </>
  );
}
