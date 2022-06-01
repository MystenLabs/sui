// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '@mysten/sui.js';

import { getEndpoint, Network } from './rpcSetting';

// TODO: Remove these types with SDK types
export type AddressOwner = { AddressOwner: string };

export type AnyVec = { vec: any[] };

export { Network, getEndpoint };

export const DefaultRpcClient = (network: Network | string) =>
    new JsonRpcProvider(getEndpoint(network));
