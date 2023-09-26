// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Dispatch, SetStateAction, useCallback, useState } from 'react';

type SetValue<T> = Dispatch<SetStateAction<T>>;

export function useLocalStorage<T>(key: string, initialValue: T): [T, SetValue<T>] {
	const getValue = useCallback(() => {
		try {
			const item = window.localStorage.getItem(key);
			return item ? (JSON.parse(item) as T) : initialValue;
		} catch (error) {
			console.warn(`Error reading localStorage key "${key}":`, error);
			return initialValue;
		}
	}, [initialValue, key]);

	const [storedValue, setStoredValue] = useState<T>(getValue);

	const setValue: SetValue<T> = useCallback(
		(value) => {
			if (typeof window === 'undefined') {
				console.warn(`Tried setting localStorage key "${key}" even though window is not defined`);
			}

			try {
				const newValue = value instanceof Function ? value(storedValue) : value;
				window.localStorage.setItem(key, JSON.stringify(newValue));
				setStoredValue(newValue);
			} catch (error) {
				console.warn(`Error setting localStorage key "${key}":`, error);
			}
		},
		[key, storedValue],
	);

	return [storedValue, setValue];
}
