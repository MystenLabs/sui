// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from "@tanstack/react-query";
import { getRawObjectParsedUnsafe } from "../rawObject";
import provider from "../provider";
import { useWalletKit } from "@mysten/wallet-kit";

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
      // Refetch every 60 seconds:
      refetchInterval: 60 * 1000,
    }
  );
}

function useObjectsOwnedByAddress() {
  const { currentAccount } = useWalletKit();
  return useQuery(
    ["owned", currentAccount],
    async () => provider.getObjectsOwnedByAddress(currentAccount!),
    {
      enabled: !!currentAccount,
      refetchInterval: 2 * 60 * 1000,
    }
  );
}

/**
 * Get all objects by type.
 */
export function useMyType<T>(type: string) {
  const { currentAccount } = useWalletKit();
  const owned = useObjectsOwnedByAddress();
  return useQuery(
    [type, currentAccount, owned.data?.length],
    async () => {
      const search = owned.data!.filter((v) => v.type.includes(type));

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
      enabled: !!currentAccount && owned.isSuccess,
      refetchInterval: 2 * 60 * 1000,
    }
  );
}
