// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { Loading } from "@/components/Loading";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { LockedListingQuery, LockedObject } from "@/types/types";
import { Locked } from "./Locked";
import { constructUrlSearchParams, getNextPageParam } from "@/utils/helpers";
import { useCurrentAccount } from "@mysten/dapp-kit";
import { TextField } from "@radix-ui/themes";
import { useState } from "react";

export function LockedList({
  enableSearch,
  params,
}: {
  isPersonal?: boolean;
  enableSearch?: boolean;
  params: LockedListingQuery;
}) {
  const account = useCurrentAccount();
  const [lockedId, setLockedId] = useState("");

  const { data, fetchNextPage, hasNextPage, isLoading, isFetchingNextPage } =
    useInfiniteQuery({
      initialPageParam: null,
      queryKey: [QueryKey.Locked, params, account?.address, lockedId],
      queryFn: async ({ pageParam }) => {
        const data = await fetch(
          CONSTANTS.apiEndpoint +
            `locked${constructUrlSearchParams({
              ...(pageParam ? { cursor: pageParam as string } : {}),
              ...(lockedId ? { objectId: lockedId } : {}),
              ...params,
            })}`,
        );
        return data.json();
      },
      select: (data) => data.pages.flatMap((page) => page.data),
      getNextPageParam,
    });

  if (isLoading) return <Loading />;
  return (
    <>
      {enableSearch && (
        <TextField.Root className="mt-3">
          <TextField.Input
            placeholder="Search by locked id"
            value={lockedId}
            onChange={(e) => setLockedId(e.target.value)}
          />
        </TextField.Root>
      )}
      <InfiniteScrollArea
        loadMore={() => fetchNextPage()}
        hasNextPage={hasNextPage}
        loading={isFetchingNextPage}
      >
        {data?.map((locked: LockedObject) => (
          <Locked key={locked.itemId} locked={locked} />
        ))}
      </InfiniteScrollArea>
    </>
  );
}
