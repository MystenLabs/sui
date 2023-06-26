// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

function useOnKeyboardEvent<K extends 'keydown' | 'keyup' | 'keypress'>(
	eventType: K,
	keys: string[],
	handler: (e: KeyboardEvent) => void,
	enabled = true,
) {
	useEffect(() => {
		if (enabled) {
			const listener = (e: KeyboardEvent) => {
				if (keys.includes(e.key)) {
					handler(e);
				}
			};

			document.addEventListener(eventType, listener);

			return () => {
				document.removeEventListener(eventType, listener);
			};
		}
	}, [eventType, keys, handler, enabled]);
}

export default useOnKeyboardEvent;
