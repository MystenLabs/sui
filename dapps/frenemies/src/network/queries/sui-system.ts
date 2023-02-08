// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  is,
  normalizeSuiAddress,
  SuiObject,
  MoveSuiSystemObjectFields,
} from "@mysten/sui.js";
import { useQuery } from "@tanstack/react-query";
import provider from "../provider";

/**
 * Address of the Sui System object.
 * Always the same in every Sui network (local, devnet, testnet).
 */
export const SUI_SYSTEM_ID: string = normalizeSuiAddress("0x5");

/**
 * Read the SuiSystem object.
 */
export function useSuiSystem() {
  return useQuery(
    ["object", SUI_SYSTEM_ID],
    async () => {
      const data = await provider.getObject(SUI_SYSTEM_ID);
      const systemObject =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === "moveObject"
          ? (data.details.data.fields as MoveSuiSystemObjectFields)
          : null;

      return systemObject;
    },
    {
      refetchInterval: 60 * 1000,
      refetchOnWindowFocus: false,
    }
  );

  // TODO: Fix raw version when there is delegated stake:
  // return useRawObject<SuiSystem>(SUI_SYSTEM_ID, SUI_SYSTEM_TYPE);
}
