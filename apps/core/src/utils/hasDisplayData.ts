// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiObjectResponse } from '@mysten/sui.js/client';

export const hasDisplayData = (obj: SuiObjectResponse) => !!obj.data?.display?.data;
