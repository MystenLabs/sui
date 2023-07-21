// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgXStroke16 = (props: SVGProps<SVGSVGElement>) => (
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
			fillRule="evenodd"
			d="M4.464 3.05A1 1 0 1 0 3.05 4.464L6.586 8 3.05 11.536a1 1 0 1 0 1.414 1.414L8 9.414l3.536 3.536a1 1 0 0 0 1.414-1.414L9.414 8l3.536-3.536a1 1 0 1 0-1.414-1.414L8 6.586 4.464 3.05Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgXStroke16;
