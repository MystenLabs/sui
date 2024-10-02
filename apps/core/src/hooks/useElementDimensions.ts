// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RefObject, useEffect, useState } from 'react';

export function useElementDimensions(
	elementRef: RefObject<HTMLElement>,
	defaultHeight: number = 0,
	defaultWidth: number = 0,
) {
	const [height, setHeight] = useState(defaultHeight);
	const [width, setWidth] = useState(defaultWidth);

	useEffect(() => {
		const resizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				const entryHeight = entry.contentRect.height;
				const entryWidth = entry.contentRect.width;

				if (entryHeight !== height) {
					setHeight(entryHeight);
				}

				if (entryWidth !== width) {
					setWidth(entryWidth);
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
	}, [elementRef, height, width]);

	return [height, width];
}
