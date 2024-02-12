// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS } from "@/constants";
import { Loading } from "@/components/Loading";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { useCurrentAccount, useSuiClientInfiniteQuery } from "@mysten/dapp-kit";
import { LockedObject } from "./LockedObject";

/**
 * Similar to the `ApiLockedList` but fetches the owned locked objects
 * but fetches the objects from the on-chain state, instead of relying on the indexer API.
 */
export function OwnedLockedList() {
  const account = useCurrentAccount();

  const { data, isLoading, fetchNextPage, hasNextPage, isFetchingNextPage } =
    useSuiClientInfiniteQuery(
      "getOwnedObjects",
      {
        filter: {
          StructType: CONSTANTS.escrowContract.lockedType,
        },
        owner: account?.address!,
        options: {
          showContent: true,
          showOwner: true,
        },
      },
      {
        enabled: !!account?.address,
        select: (data) => data.pages.flatMap((page) => page.data),
      },
    );

  if (isLoading) return <Loading />;
  return (
    <>
      <InfiniteScrollArea
        loadMore={() => fetchNextPage()}
        hasNextPage={hasNextPage}
        loading={isFetchingNextPage}
      >
        {data?.map((item) => (
          <LockedObject key={item.data?.objectId} object={item.data!} />
        ))}
      </InfiniteScrollArea>
    </>
  );
}
