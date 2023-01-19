// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { z } from "zod";
import { Network } from "@mysten/sui.js";

const ConfigSchema = z.object({
  VITE_NETWORK: z
    .union([z.nativeEnum(Network), z.string()])
    .default(Network.LOCAL),
  // TODO: Remove default
  VITE_LEADERBOARD: z.string().default(""),
});

export const config = ConfigSchema.parse(import.meta.env);
