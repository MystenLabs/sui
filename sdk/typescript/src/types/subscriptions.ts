// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Infer, number } from 'superstruct';

export const SubscriptionId = number();
export type SubscriptionId = Infer<typeof SubscriptionId>;

export type Unsubscribe = () => Promise<boolean>;
