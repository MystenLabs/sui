// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';

export const provider = new SuiClient({ url: getFullnodeUrl('testnet') });
