// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as femver from '@suchipi/femver';

export type RpcApiVersion = string;

// TODO: Explore other ways to do this, maybe an object-oriented way where there's some `version` object
// that contains functions like `gt` itself.

// Export all of the version utilities in femver to make it easier for consumers to work with versioning.
export const VersionUtils = femver;
