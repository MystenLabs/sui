// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { AccountDetails } from '../wallet-status-change';
import type { BasePayload } from '_payloads';

export interface GetAccountResponse extends BasePayload {
    type: 'get-account-response';
    accounts: AccountDetails[];
}
