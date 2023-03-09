// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Connection, JsonRpcProvider } from "@mysten/sui.js";

import { config } from "../config";

const provider = new JsonRpcProvider(
  new Connection({ fullnode: config.VITE_NETWORK })
);

export default provider;
