// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';

export function useDebouncedValue<T>(value: T, delay: number = 250): T {
	const [debouncedValue, setDebouncedValue] = useState(value);
	useEffect(() => {
		const timeout = setTimeout(() => setDebouncedValue(value), delay);
		return () => clearTimeout(timeout);
	}, [value, delay]);
	return debouncedValue;
}
