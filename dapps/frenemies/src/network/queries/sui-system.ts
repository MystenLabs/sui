// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from "@mysten/sui.js";
import { useRawObject } from "./use-raw";
import { SuiSystem } from "../types";

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
export const SUI_SYSTEM: string = normalizeSuiAddress("0x5");

/**
 * Read the SuiSystem object.
 */
export function useSuiSystem() {
  return useRawObject<SuiSystem>(SUI_SYSTEM, "sui_system::SuiSystemState");
}
