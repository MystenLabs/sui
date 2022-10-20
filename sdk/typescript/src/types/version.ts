// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const VERSION_REGEX = /^(\d+)\.(\d+)\.(\d+)$/;

export type RpcApiVersion = {
  major: number;
  minor: number;
  patch: number;
};

export function parseVersionFromString(
  version: string
): RpcApiVersion | undefined {
  const match = version.match(VERSION_REGEX);
  if (match) {
    return {
      major: Number(match[1]),
      minor: Number(match[2]),
      patch: Number(match[3]),
    };
  }
  return undefined;
}
