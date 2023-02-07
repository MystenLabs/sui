// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SystemEpochInfo, SYSTEM_EPOCH_INFO } from "../types";
import { useQuery } from "@tanstack/react-query";
import provider from "../provider";
import { bcs } from "../bcs";
import { useEffect } from "react";

/**
 * Fetch the most recent SystemEpochInfo event to get the
 * data on the current epoch and timestamps.
 */
export function useEpoch() {
  const ret = useQuery(
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
      // Refetch every 10 minutes:
      refetchInterval: 10 * 60 * 1000,
    }
  );

  // This does additional refetching in the event that the epoch changes between refetches:
  useEffect(() => {
    if (!ret.data || !ret.data.timestamp || !ret.data.prevTimestamp) return;
    const { timestamp, prevTimestamp } = ret.data;

    const interval = setInterval(() => {
      const prevEpochLength = timestamp - prevTimestamp;
      const timePassed = Date.now() - timestamp;
      const timeLeft = prevEpochLength - timePassed;
      console.log(timeLeft);
      // 1 minute after we think the epoch has changed,
      if (-1 * 60 * 1000 > timeLeft) {
        ret.refetch();
        clearInterval(interval);
      }
    }, 5000);

    return () => {
      clearInterval(interval);
    };
  }, [ret.data]);

  return ret;
}
