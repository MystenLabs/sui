// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from '@mysten/sui.js';

import { getEndpoint } from './rpcSetting';

// TODO: Remove these types with SDK types
export type AddressBytes = number[];
export type AddressOwner = { AddressOwner: AddressBytes };

export type AnyVec = { vec: any[] };
export type JsonBytes = { bytes: number[] };

export const DefaultRpcClient = new JsonRpcProvider(getEndpoint());
