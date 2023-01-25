// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { config } from "../../config";
import { useLeaderboard } from "../../network/queries/leaderboard";
import { useSuiSystem } from "../../network/queries/sui-system";

/**
 * Round number.
 *
 * Requires reading the SuiSystem object to get current epoch
 * minus the start round for the Frenemies game.
 */
export function Round() {
  const { data: system } = useSuiSystem();
  const { data: leaderboard } = useLeaderboard(config.VITE_LEADERBOARD);

  if (!system || !leaderboard) {
    return null;
  }

  const round = system.data.epoch - leaderboard.data.startEpoch;

  return (
    <h2 className="uppercase text-steel-dark font-thin text-6xl sm:text-8xl md:text-9xl lg:text-9xl xl:text-[160px] leading-tight text-center tracking-widest">
      Round {round.toString()}
    </h2>
  );
}
