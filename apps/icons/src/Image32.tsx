// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgImage32 = (props: SVGProps<SVGSVGElement>) => (
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
			d="m6.333 21.333 3.662-4.657a2.667 2.667 0 0 1 4.12-.09l3.218 3.747m-2.78-3.236c1.383-1.759 3.31-4.251 3.436-4.413a2.667 2.667 0 0 1 4.126-.098l3.218 3.747M9 25.667h14A2.667 2.667 0 0 0 25.667 23V9A2.667 2.667 0 0 0 23 6.333H9A2.667 2.667 0 0 0 6.333 9v14A2.667 2.667 0 0 0 9 25.667Z"
		/>
	</svg>
);
export default SvgImage32;
