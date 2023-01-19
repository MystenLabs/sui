// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Leaderboard } from "../types";
import { getRawObject, ObjectData } from "../rawObject";
import provider from "../provider";
import bcs from "../bcs";
import { useQuery } from "@tanstack/react-query";

/**
 * Get a shared Leaderboard object's data.
 * Its objectId should always be known and set in the environment.
 */
export function useLeaderboard(objectId: string) {
  return useQuery(
    ["leaderboard", objectId],
    async (): Promise<ObjectData<Leaderboard> | null> => {
      const objectData = await getRawObject(provider, objectId);
      const {
        reference,
        data: { bcs_bytes },
      } = objectData.details;

      return {
        reference,
        data: bcs.de("leaderboard::Leaderboard", bcs_bytes, "base64"),
      };
    }
  );
}
