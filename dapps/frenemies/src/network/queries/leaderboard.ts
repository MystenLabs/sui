// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Leaderboard } from "../types";
import { useRawObject } from "./use-raw";

/**
 * Get a shared Leaderboard object's data.
 * Its objectId should always be known and set in the environment.
 */
export function useLeaderboard(objectId: string) {
  return useRawObject<Leaderboard>(objectId, "leaderboard::Leaderboard");
}
