// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { JsonRpcProvider, Network } from "@mysten/sui.js";

const rpc: string | Network = import.meta.env.VITE_RPC || Network.LOCAL;
const provider = new JsonRpcProvider(rpc);

export default provider;
