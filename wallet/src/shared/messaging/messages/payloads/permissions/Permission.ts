// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PermissionType } from './PermissionType';
import type { SuiAddress } from '@mysten/sui.js';

//TODO: add description, name, tags
export interface Permission {
    name?: string;
    id: string;
    origin: string;
    favIcon: string | undefined;
    accounts: SuiAddress[];
    allowed: boolean | null;
    permissions: PermissionType[];
    createdDate: string;
    responseDate: string | null;
    requestMsgID: string;
}
