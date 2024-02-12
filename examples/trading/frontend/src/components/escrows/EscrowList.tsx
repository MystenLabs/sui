// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { Escrow } from "./Escrow";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { useCurrentAccount } from "@mysten/dapp-kit";
import { constructUrlSearchParams, getNextPageParam } from "@/utils/helpers";
import { ApiEscrowObject, EscrowListingQuery } from "@/types/types";
import { useState } from "react";
import { TextField } from "@radix-ui/themes";

export function EscrowList({
  params,
  enableSearch,
}: {
  params: EscrowListingQuery;
  enableSearch?: boolean;
}) {
  const account = useCurrentAccount();

  const [escrowId, setEscrowId] = useState("");

  const { data, fetchNextPage, hasNextPage, isLoading, isFetchingNextPage } =
    useInfiniteQuery({
      initialPageParam: null,
      queryKey: [QueryKey.Escrow, params, account?.address, escrowId],
      queryFn: async ({ pageParam }) => {
        const data = await fetch(
          `${CONSTANTS.apiEndpoint}escrows${constructUrlSearchParams({
            ...params,
            ...(pageParam ? { cursor: pageParam as string } : {}),
            ...(escrowId ? { objectId: escrowId } : {}),
          })}`,
        );
        return data.json();
      },
      select: (data) => data.pages.flatMap((page) => page.data),
      getNextPageParam,
    });

  return (
    <div>
      {enableSearch && (
        <TextField.Root>
          <TextField.Input
            placeholder="Search by escrow id"
            value={escrowId}
            onChange={(e) => setEscrowId(e.target.value)}
          />
        </TextField.Root>
      )}
      <InfiniteScrollArea
        loadMore={() => fetchNextPage()}
        hasNextPage={hasNextPage}
        loading={isFetchingNextPage || isLoading}
      >
        {data?.map((escrow: ApiEscrowObject) => (
          <Escrow key={escrow.itemId} escrow={escrow} />
        ))}
      </InfiniteScrollArea>
    </div>
  );
}
