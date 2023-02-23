// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { config, ROUND_OFFSET } from "../../config";
import { useEpoch } from "../../network/queries/epoch";
import { useRawObject } from "../../network/queries/use-raw";
import { LEADERBOARD, Leaderboard } from "../../network/types";

/**
 * Round number.
 *
 * Requires reading the SuiSystem object to get current epoch
 * minus the start round for the Frenemies game.
 */
export function Round() {
  const { data: epoch } = useEpoch();
  const { data: leaderboard } = useRawObject<Leaderboard>(
    config.VITE_LEADERBOARD,
    LEADERBOARD
  );

  if (!epoch || !leaderboard) {
    return null;
  }

  const round =
    BigInt(epoch.epoch) - leaderboard.data.startEpoch + ROUND_OFFSET;

  return (
    <h2 className="uppercase text-steel-dark font-thin text-6xl sm:text-8xl md:text-9xl lg:text-9xl xl:text-[160px] leading-tight text-center tracking-widest">
      Round {round.toString()}
    </h2>
  );
}
