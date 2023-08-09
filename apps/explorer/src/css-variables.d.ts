// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// eslint-disable-next-line react/no-typos
import 'react';

declare module 'react' {
	interface CSSProperties {
		[key: `--${string}`]: string | number | null;
	}
}
