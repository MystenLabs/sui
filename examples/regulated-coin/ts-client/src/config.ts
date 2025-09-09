// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { config } from "dotenv";

config({});
export const PACKAGE_ID = process.env.PACKAGE_ID!;
export const SUI_NETWORK = process.env.SUI_FULLNODE_URL!;
export const ADMIN_ADDRESS = process.env.ADMIN_ADDRESS!;
export const ADMIN_SECRET_KEY = process.env.ADMIN_SECRET_KEY!;

export const TREASURY_CAP_ID = process.env.TREASURY_CAP_ID!;
export const DENY_CAP_ID = process.env.DENY_CAP_ID!;

export const SUI_DENY_LIST_OBJECT_ID : string = '0x403';
export const MODULE_NAME : string = process.env.MODULE_NAME!;
export const COIN_NAME : string = process.env.COIN_NAME!;
export const COIN_TYPE =`${PACKAGE_ID}::${MODULE_NAME}::${COIN_NAME}`;

// console.log everything in the process.env object
const keys = Object.keys(process.env);
console.log("env contains ADMIN_ADDRESS:", keys.includes("ADMIN_ADDRESS"));
console.log("env contains ADMIN_SECRET_KEY:", keys.includes("ADMIN_SECRET_KEY"));
console.log("env contains TREASURY_CAP_ID:", keys.includes("TREASURY_CAP_ID"));
console.log("env contains DENY_CAP_ID:", keys.includes("DENY_CAP_ID"));
console.log("env contains MODULE_NAME:", keys.includes("MODULE_NAME"));
console.log("env contains COIN_NAME:", keys.includes("COIN_NAME"));