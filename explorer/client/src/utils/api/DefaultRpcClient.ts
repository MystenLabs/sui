// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { tryGetRpcSetting } from './rpcSetting';
import { JsonRpcProvider } from 'sui.js';

export type AddressBytes = number[];
export type AddressOwner = { AddressOwner: AddressBytes };

export type AnyVec = { vec: any[] };
export type JsonBytes = { bytes: number[] };

const rpcUrl = tryGetRpcSetting() ?? 'http://127.0.0.1:5001';

export const DefaultRpcClient = new JsonRpcProvider(rpcUrl);

console.log(DefaultRpcClient);
