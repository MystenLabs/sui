// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createContext } from 'react';

import type { DappKitStore } from './store/index.js';

export const DappKitStoreContext = createContext<DappKitStore | null>(null);
