// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RefObject, useEffect, useState } from 'react';

export function useElementHeight(elementRef: RefObject<HTMLElement>, defaultHeight: number) {
	const [height, setHeight] = useState(defaultHeight);

	useEffect(() => {
		const resizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				const entryHeight = entry.contentRect.height;
				if (entryHeight !== height) {
					setHeight(entry.contentRect.height);
				}
			}
		});

		if (elementRef.current) {
			resizeObserver.observe(elementRef.current);
		}

		const headerCurrentRef = elementRef.current;

		return () => {
			if (headerCurrentRef) {
				resizeObserver.unobserve(headerCurrentRef);
			}
		};
	}, [elementRef, height]);

	return height;
}
