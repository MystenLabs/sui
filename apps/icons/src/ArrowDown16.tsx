// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgArrowDown16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M8.994 3v7.586l2.29-2.293a.998.998 0 0 1 1.635 1.089 1.002 1.002 0 0 1-.223.325l-3.995 4a.999.999 0 0 1-1.414 0l-3.994-4A1.001 1.001 0 0 1 4 7.999c.265 0 .52.106.707.294l2.29 2.293V3a1 1 0 0 1 .998-1 .998.998 0 0 1 .999 1Z"
		/>
	</svg>
);
export default SvgArrowDown16;
