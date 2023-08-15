// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { KnipConfig } from 'knip';

const config: KnipConfig = {
	entry: ['src/index.tsx'],
	project: ['src/**/*.ts', 'src/**/*.tsx'],
	ignore: ['**/*.d.ts', 'src/utils/analytics/ampli/index.ts'],
};

export default config;
