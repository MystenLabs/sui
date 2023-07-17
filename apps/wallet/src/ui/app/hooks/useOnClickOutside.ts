// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import type { RefObject } from 'react';

type Event = MouseEvent | TouchEvent;

const useOnClickOutside = <T extends HTMLElement = HTMLElement>(
	ref: RefObject<T>,
	handler: (event: Event) => void,
) => {
	useEffect(() => {
		const listener = (event: Event) => {
			const el = ref?.current;
			if (!el || el.contains(event?.target as Node)) {
				return;
			}

			handler(event); // Call the handler only if the click is outside of the element passed.
		};

		document.addEventListener('click', listener, true);
		document.addEventListener('touchstart', listener, true);

		return () => {
			document.removeEventListener('click', listener, true);
			document.removeEventListener('touchstart', listener, true);
		};
	}, [ref, handler]); // Reload only if ref or handler changes
};

export default useOnClickOutside;
