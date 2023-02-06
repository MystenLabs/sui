// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SystemEpochInfo, SYSTEM_EPOCH_INFO } from "../types";
import { useQuery } from "@tanstack/react-query";
import provider from "../provider";
import { bcs } from "../bcs";
import { config } from "../../config";

/**
 * Fetch the most recent SystemEpochInfo event to get the
 * data on the current epoch and timestamps.
 */
export function useEpoch() {
  return useQuery(
    ["epoch"],
    async (): Promise<{
      timestamp: number;
      prevTimestamp: number;
      data: SystemEpochInfo;
    } | null> => {
      const { data } = await provider.getEvents(
        { MoveEvent: SYSTEM_EPOCH_INFO },
        null,
        2,
        "descending"
      );
      const [evt, prevEvt] = data;

      // should never happen; it's a platform requirement.
      if (data.length == 0 || !("moveEvent" in evt.event)) {
        return null;
      }

      return {
        timestamp: evt.timestamp,
        prevTimestamp: prevEvt?.timestamp || 0,
        data: bcs.de(SYSTEM_EPOCH_INFO, evt.event.moveEvent.bcs, "base64"),
      };
    },
    {
      // refetch 4 times per epoch
      refetchInterval: (+config.VITE_EPOCH_LEN * 60000) / 4,
    }
  );
}
