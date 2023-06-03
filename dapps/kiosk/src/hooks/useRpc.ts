// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, testnetConnection } from '@mysten/sui.js';

// const provider = new JsonRpcProvider(new Connection({ fullnode: 'http://localhost:9000' }));
/**
 * Provides you with a configured JSON RPC Provider.
 * We expose this over a hook so that it can be dynamic if needed (e.g. change based on network)
 */
export function useRpc(): JsonRpcProvider {
  return new JsonRpcProvider(testnetConnection);
}
