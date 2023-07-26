// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgArrowDown12 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 12 12"
		{...props}
	>
		<path
			fill="currentColor"
			d="M6.595 2v6.068l1.831-1.834a.799.799 0 1 1 1.13 1.131l-3.196 3.2a.798.798 0 0 1-1.13 0l-3.196-3.2a.8.8 0 0 1 1.13-1.13l1.833 1.833V2a.8.8 0 0 1 1.364-.566.8.8 0 0 1 .234.566Z"
		/>
	</svg>
);
export default SvgArrowDown12;
