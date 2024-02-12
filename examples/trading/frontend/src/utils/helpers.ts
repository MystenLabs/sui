// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { CONSTANTS } from "@/constants";

// SPDX-License-Identifier: Apache-2.0
export const constructUrlSearchParams = (
  object: Record<string, string>,
): string => {
  const searchParams = new URLSearchParams();

  for (const key in object) {
    searchParams.set(key, object[key]);
  }

  return `?${searchParams.toString()}`;
};

/** A naive way to understand whether we have a next page or not */
export const getNextPageParam = (lastPage: any) => {
  if ("api" in lastPage) {
    return lastPage.api.data.length < CONSTANTS.apiPageLimit
      ? undefined
      : lastPage.api.cursor;
  }
  return lastPage.data.length < CONSTANTS.apiPageLimit
    ? undefined
    : lastPage.cursor;
};
