// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MutableRefObject, useEffect, useState } from 'react';

export const useOnScreen = (elementRef: MutableRefObject<Element | null>) => {
	const [isIntersecting, setIsIntersecting] = useState(false);

	useEffect(() => {
		const node = elementRef.current;
		if (!node) return;

		const observer = new IntersectionObserver(
			([entry]: IntersectionObserverEntry[]): void => {
				setIsIntersecting(entry.isIntersecting);
			},
			{ threshold: 0.01 },
		);
		observer.observe(node);
		return () => observer.disconnect();
	}, [elementRef]);

	return { isIntersecting };
};
