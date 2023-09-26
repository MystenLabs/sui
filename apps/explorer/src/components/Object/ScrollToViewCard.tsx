// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, type ReactNode } from 'react';

interface ScrollToViewCardProps {
	children: ReactNode;
	inView: boolean;
}

export function ScrollToViewCard({ children, inView }: ScrollToViewCardProps) {
	const scrollViewRef = useRef<HTMLDivElement | null>(null);

	useEffect(() => {
		if (!scrollViewRef?.current || !inView) return;

		const parentNode = scrollViewRef.current.parentNode as HTMLDivElement;
		const parentOffset = parentNode.offsetTop;
		const currentOffset = scrollViewRef.current.offsetTop;

		const elementOffset = currentOffset - parentOffset;

		parentNode.scrollTo({
			top: elementOffset,
			behavior: 'smooth',
		});
	}, [inView, scrollViewRef]);

	return <div ref={scrollViewRef}>{children}</div>;
}
