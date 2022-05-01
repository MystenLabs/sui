// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from 'sui.js';

import { tryGetRpcSetting } from './rpcSetting';

export type AddressBytes = number[];
export type AddressOwner = { AddressOwner: AddressBytes };

export type AnyVec = { vec: any[] };
export type JsonBytes = { bytes: number[] };

const useLocal = true;
const LOCAL = 'http://127.0.0.1:5001';
const DEVNET = 'http://34.105.36.61:9000';

const rpcUrl = tryGetRpcSetting() ?? useLocal ? LOCAL : DEVNET;

export const DefaultRpcClient = new JsonRpcProvider(rpcUrl);
