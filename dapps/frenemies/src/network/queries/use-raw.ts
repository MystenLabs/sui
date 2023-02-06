// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from "@tanstack/react-query";
import { getRawObjectParsedUnsafe } from "../rawObject";
import provider from "../provider";

/**
 * Generic method to fetch a RawObject from the network.
 */
export function useRawObject<T>(objectId: string, bcsType: string) {
  return useQuery(
    [bcsType, objectId],
    async () => {
      return getRawObjectParsedUnsafe<T>(provider, objectId, bcsType);
    },
    {
      // Refetch every 10 seconds:
      refetchInterval: 10 * 1000,
    }
  );
}

/**
 * Get all objects by type.
 */
export function useMyType<T>(type: string, account?: string | null) {
  return useQuery(
    [type, account],
    async () => {
      if (!account) {
        return null;
      }

      const objects = await provider.getObjectsOwnedByAddress(account);
      const search = objects.filter((v) => v.type.includes(type));

      if (!search.length) {
        return null;
      }

      return Promise.all(
        search.map((obj) =>
          getRawObjectParsedUnsafe<T>(provider, obj.objectId, type)
        )
      );
    },
    {
      enabled: !!account,
      refetchInterval: 2 * 60 * 1000,
    }
  );
}
