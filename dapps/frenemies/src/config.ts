// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { z } from "zod";

// Oops, we need to bump the round.
export const ROUND_OFFSET = 5n;

export const GAME_END_DATE = new Date(
  "Tue Feb 14 2023 10:00:00 GMT-0800 (Pacific Standard Time)"
);

export const gameIsOver = () => Date.now() >= GAME_END_DATE.getTime();

const ConfigSchema = z.object({
  /** Fullnode RPC URL to use for the dapp */
  VITE_NETWORK: z.string(),
  /** Leaderboard object: shared, contains information about 1000 top players */
  VITE_LEADERBOARD: z.string(),
  /** Name Registry: shared, used when signing up (and getting a Scorecard) */
  VITE_REGISTRY: z.string(),
  /** Frenemies Package ID */
  VITE_PKG: z.string(),
  VITE_MIGRATION: z.string(),
  /** Package for the previous version of frenemies: */
  VITE_OLD_PKG: z.string(),
  /** Registry for the previous version of frenemies: */
  VITE_OLD_REGISTRY: z.string(),
  /** The noop package */
  VITE_NOOP: z.string().default("0x7829fea9bbd3aecdc7721465789c5431bdaf9436"),
});

export const config = ConfigSchema.parse(import.meta.env);
