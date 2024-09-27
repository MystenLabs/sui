// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useInfiniteQuery } from "@tanstack/react-query";
import { CONSTANTS, QueryKey } from "@/constants";
import { InfiniteScrollArea } from "@/components/InfiniteScrollArea";
import { ApiLockedObject, LockedListingQuery } from "@/types/types";
import { constructUrlSearchParams, getNextPageParam } from "@/utils/helpers";
import { useSuiClient } from "@mysten/dapp-kit";
import { TextField } from "@radix-ui/themes";
import { useState } from "react";
import { LockedObject } from "./LockedObject";
import { useGetLockedObject } from "@/hooks/useGetLockedObject";

/**
 * Fetches all the non-deleted system `Locked` objects from the API in a paginated fashion.
 * Then, it proceeds into fetching the on-chain state, so we can better trust the latest
 * state of the object in regards to ownership.
 *
 * We do this because `Locked` object has `store` ability, so that means that the `creator` field
 * from the API could be stale.
 */
export function LockedList({
  enableSearch,
  params,
}: {
  isPersonal?: boolean;
  enableSearch?: boolean;
  params: LockedListingQuery;
}) {
  const [lockedId, setLockedId] = useState("");
  const suiClient = useSuiClient();

  const { data: searchData } = useGetLockedObject({
    lockedId,
  });

  const { data, fetchNextPage, hasNextPage, isLoading, isFetchingNextPage } =
    useInfiniteQuery({
      initialPageParam: null,
      queryKey: [QueryKey.Locked, params, lockedId],
      queryFn: async ({ pageParam }) => {
        /*
         * Fetch the locked objects from the API.
         */
        const data = await (
          await fetch(
            CONSTANTS.apiEndpoint +
              "locked" +
              constructUrlSearchParams({
                deleted: "false",
                ...(pageParam ? { cursor: pageParam as string } : {}),
                ...(params || {}),
              }),
          )
        ).json();

        /*
         * Use the objectIds from the API to fetch the on-chain state. This is done to ensure that
         * the ownership of each object is up-to-date.
         */
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
      select: (data) => data.pages,
      getNextPageParam,
      enabled: !lockedId,
    });

  /**
   * Returns all `Locked` objects or the one that matches the search query if it exists.
   */
  const suiObjects = () => {
    if (lockedId) {
      if (
        !searchData?.data?.type?.startsWith(CONSTANTS.escrowContract.lockedType)
      )
        return [];
      return [searchData?.data!];
    }
    return data?.flatMap((x) => x.suiObjects) || [];
  };

  const apiData = () => {
    return data?.flatMap((x) => x.api.data);
  };

  // Find the itemID from the API request to skip fetching the DF on-chain.
  // We can always be certain that the itemID can't change for a given `Locked` object.
  const getItemId = (objectId?: string) => {
    return apiData()?.find((x) => x.objectId === objectId)?.itemId;
  };

  return (
    <>
      {enableSearch && (
        <TextField.Root
          className="mt-3"
          placeholder="Search by locked id"
          value={lockedId}
          onChange={(e) => setLockedId(e.target.value)}
        ></TextField.Root>
      )}
      <InfiniteScrollArea
        loadMore={() => fetchNextPage()}
        hasNextPage={hasNextPage}
        loading={isFetchingNextPage || isLoading}
      >
        {suiObjects().map((object) => (
          <LockedObject
            key={object?.objectId!}
            object={object!}
            itemId={getItemId(object?.objectId)}
          />
        ))}
      </InfiniteScrollArea>
    </>
  );
}
