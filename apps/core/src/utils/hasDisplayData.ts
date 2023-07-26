// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectDisplay } from '@mysten/sui.js';
import { SuiObjectResponse } from '@mysten/sui.js/client';

export const hasDisplayData = (obj: SuiObjectResponse) => !!getObjectDisplay(obj).data;
