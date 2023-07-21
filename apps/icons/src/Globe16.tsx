// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgGlobe16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12Z"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M10.5 8c0 3.724-1.544 6-2.5 6s-2.5-2.276-2.5-6S7.044 2 8 2s2.5 2.276 2.5 6ZM2 8h12"
		/>
	</svg>
);
export default SvgGlobe16;
