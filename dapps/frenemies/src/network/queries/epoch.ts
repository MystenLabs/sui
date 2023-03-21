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
      epoch: number;
      timestamp: number;
      prevTimestamp: number;
      data: SystemEpochInfo;
    } | null> => {
      const { data } = await provider.getEvents({
        query: { MoveEvent: SYSTEM_EPOCH_INFO },
        limit: 2,
        order: "descending",
      });
      const [evt, prevEvt] = data;

      // should never happen; it's a platform requirement.
      if (data.length == 0 || !("moveEvent" in evt.event)) {
        return null;
      }

      const eventData = bcs.de(
        SYSTEM_EPOCH_INFO,
        evt.event.moveEvent.bcs,
        "base64"
      ) as SystemEpochInfo;

      return {
        data: eventData,
        epoch: Number(eventData.epoch),
        timestamp: evt.timestamp,
        prevTimestamp: prevEvt?.timestamp || 0,
      };
    },
    {
      // Refetch every 10 minutes:
      refetchInterval: 10 * 60 * 1000,
      staleTime: 10 * 60 * 1000,
    }
  );

  // This does additional refetching one minute after we think the epoch changes:
  useEffect(() => {
    if (!ret.data || !ret.data.timestamp || !ret.data.prevTimestamp) return;
    const { timestamp, prevTimestamp } = ret.data;
    const timePassed = Date.now() - timestamp;
    const prevEpochLength = timestamp - prevTimestamp;
    const timeLeft = prevEpochLength - timePassed;

    const timeout = setTimeout(() => {
      ret.refetch();
    }, timeLeft + 60 * 1000);

    return () => {
      clearTimeout(timeout);
    };
  }, [ret.data]);

  return ret;
}
