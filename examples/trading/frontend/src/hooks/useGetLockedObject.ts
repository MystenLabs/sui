// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientQuery } from "@mysten/dapp-kit";

/**
 * A re-usable hook for querying a locked object by ID
 * from the on-chain state.
 */
export function useGetLockedObject({ lockedId }: { lockedId: string }) {
  return useSuiClientQuery(
    "getObject",
    {
      id: lockedId,
      options: {
        showType: true,
        showOwner: true,
        showContent: true,
      },
    },
    {
      enabled: !!lockedId,
    },
  );
}
