// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export type RpcApiVersion = {
  major: number;
  minor: number;
  patch: number;
};

export function parseVersionFromString(
  version: string
): RpcApiVersion | undefined {
  const versions = version.split('.');
  return {
    major: parseInt(versions[0], 10),
    minor: parseInt(versions[1], 10),
    patch: parseInt(versions[2], 10),
  };
}
