// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SYSTEM_EPOCH_INFO } from "../types";
import { useQuery } from "@tanstack/react-query";
import provider from "../provider";
import { bcs } from "../bcs";
import { config } from "../../config";

/**
 * Fetch the most recent SystemEpochInfo event to get the
 * data on the current epoch and timestamps.
 */
export function useEpoch() {
  return useQuery(["epoch"], async () => {
    const { data } = await provider.getEvents(
      { MoveEvent: SYSTEM_EPOCH_INFO },
      null,
      1,
      "descending"
    );
    const [evt] = data;

    // should never happen; it's a platform requirement.
    if (data.length == 0 || !("moveEvent" in evt.event)) {
      return null;
    }

    return {
      timestamp: evt.timestamp,
      data: bcs.de(SYSTEM_EPOCH_INFO, evt.event.moveEvent.bcs, "base64"),
    };
  }, {
    // refetch twice per epoch
    refetchInterval: +config.VITE_EPOCH_LEN * 60000 / 2
  });
}
