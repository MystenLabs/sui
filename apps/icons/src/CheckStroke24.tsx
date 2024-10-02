// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgCheckStroke24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path
			fill="currentColor"
			fillRule="evenodd"
			d="M19.55 4.32a.875.875 0 0 1 .13 1.23l-10.5 13a.875.875 0 0 1-1.33.035l-4.5-5a.875.875 0 0 1 1.3-1.17l3.814 4.237L18.319 4.45a.875.875 0 0 1 1.23-.13Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgCheckStroke24;
