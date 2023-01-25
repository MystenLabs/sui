// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import {  }
import provider from "../provider";
import { useQuery } from "@tanstack/react-query";
import { config } from "../../config";
import { bcs } from "@mysten/sui.js";
import { ScorecardUpdatedEvent } from "../types";

/**
 * Event signature that we're looking for.
 */
const EVT_TYPE = `${config.VITE_PKG}::frenemies::ScorecardUpdateEvent`;

/**
 * Get Scorecard activity by fetching all txs involving a Scorecard object.
 * There're two types of transactions we're searching: `register` and `update`
 * move entry function calls. The latter produces ScorecardUpdated event.
 *
 * @param scorecardId
 * @returns
 */
export function useScorecardHistory(scorecardId: string | null | undefined) {
  return useQuery(["scorecard-history", scorecardId], async () => {
    if (!scorecardId) {
      return null;
    }

    const txIds = await provider.getTransactionsForObject(scorecardId);
    const txs = await provider.getTransactionWithEffectsBatch(txIds);

    return txs
      .reduce((acc: any[], tx) => acc.concat(tx.effects.events || []), [])
      .filter((evt) => "moveEvent" in evt && evt.moveEvent.type == EVT_TYPE)
      .map<ScorecardUpdatedEvent>(({ moveEvent }) =>
        bcs.de("frenemies::ScorecardUpdateEvent", moveEvent.bcs, "base64")
      );
  });
}
