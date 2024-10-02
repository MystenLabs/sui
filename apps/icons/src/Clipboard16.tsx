// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgClipboard16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 18"
		{...props}
	>
		<path
			stroke="#A0B6C3"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M4 3H2.931A1.931 1.931 0 0 0 1 4.931v10.138C1 16.135 1.865 17 2.931 17h10.138A1.931 1.931 0 0 0 15 15.069V4.931A1.931 1.931 0 0 0 13.069 3H12"
		/>
		<path
			stroke="#A0B6C3"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M4 2.5A1.5 1.5 0 0 1 5.5 1h5A1.5 1.5 0 0 1 12 2.5v1A1.5 1.5 0 0 1 10.5 5h-5A1.5 1.5 0 0 1 4 3.5v-1Z"
		/>
	</svg>
);
export default SvgClipboard16;
