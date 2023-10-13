// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgClipboard24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M6.655 6C5.741 6 5 6.865 5 7.931v9.138C5 18.135 5.741 19 6.655 19h8.69c.914 0 1.655-.865 1.655-1.931V7.931C17 6.865 16.259 6 15.345 6"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M7 5.625C7 5.004 7.672 4.5 8.5 4.5h5c.828 0 1.5.504 1.5 1.125v.75c0 .621-.672 1.125-1.5 1.125h-5C7.672 7.5 7 6.996 7 6.375v-.75Z"
		/>
	</svg>
);
export default SvgClipboard24;
