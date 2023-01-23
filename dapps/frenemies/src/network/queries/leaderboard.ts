// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Leaderboard } from "../types";
import { getRawObjectParsedUnsafe, ObjectData } from "../rawObject";
import provider from "../provider";
import { useQuery } from "@tanstack/react-query";

/**
 * Get a shared Leaderboard object's data.
 * Its objectId should always be known and set in the environment.
 */
export function useLeaderboard(objectId: string) {
  return useQuery(
    ["leaderboard", objectId],
    async (): Promise<ObjectData<Leaderboard>> => {
      return getRawObjectParsedUnsafe(provider, objectId, "leaderboard::Leaderboard");
    }
  );
}
