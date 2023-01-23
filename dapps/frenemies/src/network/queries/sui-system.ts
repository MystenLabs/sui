// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from "@mysten/sui.js";
import { useQuery } from "@tanstack/react-query";
import provider from "../provider";
import { getRawObjectParsed, ObjectData } from "../rawObject";
import { SuiSystem } from "../types";

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
const SUI_SYSTEM: string = normalizeSuiAddress("0x5");

/**
 * Read the SuiSystem object.
 */
export function useSuiSystem() {
  return useQuery(["sui-system"], async (): Promise<ObjectData<SuiSystem> | null> => {
    return getRawObjectParsed(provider, SUI_SYSTEM, "sui_system::SuiSystemState");
  });
}
