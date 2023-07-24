// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgDomain32 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 32 32"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M16 25.667c5.339 0 9.667-4.328 9.667-9.667A9.667 9.667 0 0 0 16 6.333c-5.339 0-9.667 4.328-9.667 9.667 0 5.339 4.328 9.667 9.667 9.667Z"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M20.333 16c0 6-2.676 9.667-4.333 9.667S11.667 22 11.667 16 14.343 6.333 16 6.333 20.333 10 20.333 16ZM6.667 16h18.666"
		/>
	</svg>
);
export default SvgDomain32;
