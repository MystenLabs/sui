// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, type MutableRefObject, useRef } from 'react';

export const useOnScreen = (ref: MutableRefObject<Element | null>) => {
	const [isIntersecting, setIsIntersecting] = useState(false);

	const observerRef = useRef<IntersectionObserver>();
	if (!observerRef.current) {
		observerRef.current = new IntersectionObserver(
			([entry]) => setIsIntersecting(entry.isIntersecting),
			{
				threshold: [0.01],
			},
		);
	}

	useEffect(() => {
		const currObserver = observerRef.current;

		if (ref.current && currObserver) {
			currObserver.observe(ref.current);
		}

		return () => {
			currObserver && currObserver.disconnect();
		};
	});

	return { isIntersecting };
};
