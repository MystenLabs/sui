// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { z } from "zod";
import { Network } from "@mysten/sui.js";

const ConfigSchema = z.object({
  VITE_NETWORK: z
    .union([z.nativeEnum(Network), z.string()])
    .default(Network.LOCAL),
  /** Leaderboard object: shared, contains information about 1000 top players */
  VITE_LEADERBOARD: z.string(),
  /** Name Registry: shared, used when signing up (and getting a Scorecard) */
  VITE_REGISTRY: z.string(),
  /** Frenemies Package ID */
  VITE_PKG: z.string(),
  /** Package for the previous version of frenemies: */
  VITE_LEGACY_PKG: z.string(),
  /** Registry for the previous version of frenemies: */
  VITE_LEGACY_REGISTRY: z.string(),
});

export const config = ConfigSchema.parse(import.meta.env);
