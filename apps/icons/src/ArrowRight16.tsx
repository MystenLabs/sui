// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgArrowRight16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M3 8.993h7.586l-2.293 2.29a.999.999 0 1 0 1.414 1.412l4-3.994a.999.999 0 0 0 0-1.414l-4-3.995A1.001 1.001 0 0 0 7.999 4c0 .265.106.519.294.706l2.293 2.29H3a1 1 0 0 0-1 .999.998.998 0 0 0 1 .998Z"
		/>
	</svg>
);
export default SvgArrowRight16;
