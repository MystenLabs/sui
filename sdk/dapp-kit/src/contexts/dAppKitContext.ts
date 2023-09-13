// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createContext } from 'react';
import type { DAppKitStore } from '../store.js';

export const DAppKitContext = createContext<DAppKitStore | null>(null);
