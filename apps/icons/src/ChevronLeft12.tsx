// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgChevronLeft12 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 12 12"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M7 3 4 6l3 3"
		/>
	</svg>
);
export default SvgChevronLeft12;
