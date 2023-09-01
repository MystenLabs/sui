// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, type ReactNode, type MutableRefObject } from 'react';

interface ScrollToViewCardProps {
	children: ReactNode;
	inView: boolean;
	containerRef: MutableRefObject<HTMLDivElement | null>;
}

export function ScrollToViewCard({ children, inView, containerRef }: ScrollToViewCardProps) {
	const scrollViewRef = useRef<HTMLDivElement | null>(null);

	useEffect(() => {
		if (!scrollViewRef?.current || !containerRef.current || !inView) return;

		const elementOffset = scrollViewRef.current.offsetTop - (containerRef.current.offsetTop || 0);

		containerRef.current.scrollTo({
			top: elementOffset,
			behavior: 'smooth',
		});
	}, [containerRef, inView, scrollViewRef]);

	return <div ref={scrollViewRef}>{children}</div>;
}
