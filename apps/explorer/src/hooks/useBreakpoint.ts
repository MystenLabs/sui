// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMediaQuery } from '~/hooks/useMediaQuery';

/**
 * values taken from tailwind.config.js
 */
export const BREAK_POINT = {
	sm: 640,
	md: 768,
	lg: 1024,
	xl: 1280,
	'2xl': 1536,
};

export function useBreakpoint(breakpoint: keyof typeof BREAK_POINT) {
	return useMediaQuery(`(min-width: ${BREAK_POINT[breakpoint]}px)`);
}
