// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider } from 'sui.js';

import { tryGetRpcSetting } from './rpcSetting';

const useLocal = false;
const LOCAL = 'http://127.0.0.1:5001';
const DEVNET = 'https://gateway.devnet.sui.io:9000';

let rpcUrl;
if(useLocal)
    rpcUrl = LOCAL;
else
    rpcUrl = tryGetRpcSetting() ?? DEVNET;

export const DefaultRpcClient = new JsonRpcProvider(rpcUrl);
