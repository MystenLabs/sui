// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from "@mysten/sui.js";
import { useQuery } from "@tanstack/react-query";
import provider from "../provider";

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
export const SUI_SYSTEM_ID: string = normalizeSuiAddress("0x5");

export function convertToString(raw?: number[] | null) {
  if (!raw) return null;
  return String.fromCharCode(...raw);
}

export function useValidators() {
  return useQuery(
    ["validators"],
    async () => {
      return provider.getValidators();
    },
    {
      refetchInterval: 60 * 1000,
      staleTime: 5000,
    }
  );
}
