// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectResponse, getObjectDisplay } from '@mysten/sui.js';

export const hasDisplayData = (obj: SuiObjectResponse) => !!getObjectDisplay(obj).data;
