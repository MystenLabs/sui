// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { Loading } from "@/components/Loading";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { ApiLockedObject, LockedListingQuery } from "@/types/types";
import { constructUrlSearchParams, getNextPageParam } from "@/utils/helpers";
import { useCurrentAccount, useSuiClient } from "@mysten/dapp-kit";
import { TextField } from "@radix-ui/themes";
import { useState } from "react";
import { LockedObject } from "./LockedObject";

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
  const suiClient = useSuiClient();

  const { data, fetchNextPage, hasNextPage, isLoading, isFetchingNextPage } =
    useInfiniteQuery({
      initialPageParam: null,
      queryKey: [QueryKey.Locked, params, account?.address, lockedId],
      queryFn: async ({ pageParam }) => {
        const data = await (
          await fetch(
            CONSTANTS.apiEndpoint +
              "locked" +
              constructUrlSearchParams({
                deleted: "false",
                ...(pageParam ? { cursor: pageParam as string } : {}),
                ...(lockedId ? { objectId: lockedId } : {}),
              }),
          )
        ).json();

        console.log(data);

        const objects = await suiClient.multiGetObjects({
          ids: data.data.map((x: ApiLockedObject) => x.objectId),
          options: {
            showOwner: true,
            showContent: true,
          },
        });
        return {
          suiObjects: objects.map((x) => x.data),
          api: data,
        };
      },
      select: (data) => data.pages.flatMap((page) => page.suiObjects),
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
        {data?.map((object) => (
          <LockedObject key={object?.objectId!} object={object!} />
        ))}
      </InfiniteScrollArea>
    </>
  );
}
