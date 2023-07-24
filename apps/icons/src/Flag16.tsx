// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgFlag16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<path
			fill="currentColor"
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M3.833 12.833v-4 4Zm0-4v-5s1.834-1.5 4.167 0c2.333 1.5 4.167 0 4.167 0v5s-1.834 1.5-4.167 0c-2.333-1.5-4.167 0-4.167 0Z"
		/>
	</svg>
);
export default SvgFlag16;
