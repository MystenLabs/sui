// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { Loading } from "@/components/Loading";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { LockedObject } from "@/types/types";
import { Locked } from "./Locked";
import { constructUrlSearchParams, getNextPageParam } from "@/utils/helpers";
import { useCurrentAccount } from "@mysten/dapp-kit";

export function LockedList({ isPersonal = false }: { isPersonal?: boolean }) {
  const account = useCurrentAccount();
  const { data, fetchNextPage, hasNextPage, isLoading, isFetchingNextPage } =
    useInfiniteQuery({
      initialPageParam: null,
      queryKey: [QueryKey.Locked, isPersonal, account?.address],
      queryFn: async ({ pageParam }) => {
        const data = await fetch(
          CONSTANTS.apiEndpoint +
            `locked${constructUrlSearchParams({
              deleted: "false",
              ...(pageParam ? { cursor: pageParam as string } : {}),
              ...(isPersonal ? { creator: account?.address } : {}),
            })}`,
        );
        return data.json();
      },
      select: (data) => data.pages.flatMap((page) => page.data),
      getNextPageParam,
    });

  if (isLoading) return <Loading />;
  return (
    <InfiniteScrollArea
      loadMore={() => fetchNextPage()}
      hasNextPage={hasNextPage}
      loading={isFetchingNextPage}
    >
      {data?.map((locked: LockedObject) => (
        <Locked key={locked.itemId} locked={locked} />
      ))}
    </InfiniteScrollArea>
  );
}
