// Copyright (c) Mysten Labs, Inc.
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

/** Checks whether we have a next page */
export const getNextPageParam = (lastPage: any) => {
  if ("api" in lastPage) {
    return lastPage.api.cursor;
  }
  return lastPage.cursor;
};
