// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

declare namespace NodeJS {
	interface ProcessEnv {
		readonly NODE_ENV: 'development' | 'production' | 'test' | undefined;
	}
}
