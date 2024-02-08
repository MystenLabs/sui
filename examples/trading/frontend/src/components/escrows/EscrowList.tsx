// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { Loading } from "@/components/Loading";
import { Escrow } from "./Escrow";
import { InfiniteScrollArea } from "../InfiniteScrollArea";
import { useCurrentAccount } from "@mysten/dapp-kit";
import { getNextPageParam } from "@/utils/helpers";

export function EscrowList() {
  const account = useCurrentAccount();

  const {
    data,
    fetchNextPage,
    hasNextPage,
    isLoading,
    isFetchingNextPage,
    refetch,
  } = useInfiniteQuery({
    initialPageParam: null,
    queryKey: [QueryKey.Escrow, account?.address],
    queryFn: async ({ pageParam }) => {
      const urlParams = new URLSearchParams();
      if (pageParam) urlParams.set("cursor", pageParam);
      urlParams.set("sender", account?.address!);
      urlParams.set("cancelled", "false");
      urlParams.set("swapped", "false");

      const data = await fetch(
        `${CONSTANTS.apiEndpoint}escrows?${urlParams.toString()}`,
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
      {data?.map((escrow: Escrow) => (
        <Escrow key={escrow.itemId} escrow={escrow} refetch={() => refetch()} />
      ))}
    </InfiniteScrollArea>
  );
}
