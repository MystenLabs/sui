// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export interface PlaceholderProps {
	width?: string;
	height?: string;
}

export function Placeholder({ width = '100%', height = '1em' }: PlaceholderProps) {
	return (
		<div
			className="h-[1em] w-full animate-shimmer rounded-[3px] bg-placeholderShimmer bg-[length:1000px_100%]"
			style={{ width, height }}
		/>
	);
}
