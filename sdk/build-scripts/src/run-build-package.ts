#! /usr/bin/env tsx
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { buildPackage } from './build-package';

buildPackage().catch((error) => {
	console.error(error);
	process.exit(1);
});
