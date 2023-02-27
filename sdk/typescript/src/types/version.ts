// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { parse } from '@suchipi/femver';

export type RpcApiVersion = {
  major: number;
  minor: number;
  patch: number;
};

export function parseVersionFromString(
  version: string,
): RpcApiVersion | undefined {
  return parse(version);
}

export function versionToString(version: RpcApiVersion): string {
  const { major, minor, patch } = version;
  return `${major}.${minor}.${patch}`;
}
