// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from "@mysten/sui.js";
import { useRawObject } from "./use-raw";
import { SuiSystem, SUI_SYSTEM as SUI_SYSTEM_TYPE } from "../types";

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
export const SUI_SYSTEM_ID: string = normalizeSuiAddress("0x5");

/**
 * Read the SuiSystem object.
 */
export function useSuiSystem() {
  return useRawObject<SuiSystem>(SUI_SYSTEM_ID, SUI_SYSTEM_TYPE);
}
