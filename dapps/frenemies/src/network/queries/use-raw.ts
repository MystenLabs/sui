// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from "@tanstack/react-query";
import { getRawObjectParsedUnsafe } from "../rawObject";
import provider from "../provider";

/**
 * Generic method to fetch a RawObject from the network.
 */
export function useRawObject<T>(objectId: string, bcsType: string) {
  return useQuery([bcsType, objectId], async () => {
    return getRawObjectParsedUnsafe<T>(provider, objectId, bcsType);
  });
}
