// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgAudio32 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M9.333 25.667a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M12.333 22.667V9A2.667 2.667 0 0 1 15 6.333h8A2.667 2.667 0 0 1 25.667 9v9.667"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M22.667 21.667a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"
		/>
	</svg>
);
export default SvgAudio32;
