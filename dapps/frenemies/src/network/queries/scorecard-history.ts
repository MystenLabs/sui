// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { bcs } from "../bcs";
import provider from "../provider";
import { useQuery } from "@tanstack/react-query";
import { ScorecardUpdatedEvent, SCORECARD_UPDATED } from "../types";

/**
 * Get Scorecard activity by fetching all txs involving a Scorecard object.
 * There're two types of transactions we're searching: `register` and `update`
 * move entry function calls. The latter produces ScorecardUpdated event.
 *
 * @param scorecardId
 * @returns
 */
export function useScorecardHistory(scorecardId?: string | null) {
  return useQuery(
    ["scorecard-history", scorecardId],
    async () => {
      if (!scorecardId) {
        return null;
      }

      // It's very likely to have duplicates in the `txIds`; so we need to
      // filter them out and pass a unique set.
      const txIds = await provider.getTransactionsForObject(scorecardId);
      const txs = await provider.getTransactionWithEffectsBatch(
        Array.from(new Set(txIds))
      );

      return txs
        .reduce((acc: any[], tx) => acc.concat(tx.effects.events || []), [])
        .filter(
          (evt) => "moveEvent" in evt && evt.moveEvent.type == SCORECARD_UPDATED
        )
        .map<ScorecardUpdatedEvent>(({ moveEvent }) =>
          bcs.de(SCORECARD_UPDATED, moveEvent.bcs, "base64")
        );
    },
    {
      enabled: !!scorecardId,
      refetchOnWindowFocus: false,
      refetchInterval: 60 * 1000,
    }
  );
}
