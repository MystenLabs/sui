// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { Escrow } from "./Escrow";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { useCurrentAccount } from "@mysten/dapp-kit";
import { constructUrlSearchParams, getNextPageParam } from "@/utils/helpers";

export function EscrowList({
  sent,
  received,
}: {
  sent?: boolean;
  received?: boolean;
}) {
  const account = useCurrentAccount();

  const { data, fetchNextPage, hasNextPage, isLoading, isFetchingNextPage } =
    useInfiniteQuery({
      initialPageParam: null,
      queryKey: [QueryKey.Escrow, account?.address, sent, received],
      queryFn: async ({ pageParam }) => {
        const data = await fetch(
          `${CONSTANTS.apiEndpoint}escrows${constructUrlSearchParams({
            cancelled: "false",
            swapped: "false",
            ...(pageParam ? { cursor: pageParam as string } : {}),
            ...(sent ? { sender: account?.address } : {}),
            ...(received ? { recipient: account?.address } : {}),
          })}`,
        );
        return data.json();
      },
      select: (data) => data.pages.flatMap((page) => page.data),
      getNextPageParam,
    });

  return (
    <InfiniteScrollArea
      loadMore={() => fetchNextPage()}
      hasNextPage={hasNextPage}
      loading={isFetchingNextPage || isLoading}
    >
      {data?.map((escrow: Escrow) => (
        <Escrow key={escrow.itemId} escrow={escrow} />
      ))}
    </InfiniteScrollArea>
  );
}
