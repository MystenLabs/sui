// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from '@mysten/sui.js/src/client';
import { createContext } from 'react';

export const EffectsPreviewContext = createContext<SuiClient | undefined>(undefined);
