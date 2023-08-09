// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgXFill16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M8 16A8 8 0 1 0 8 0a8 8 0 0 0 0 16Zm2.668-11.333a.666.666 0 0 1 .47 1.136L8.946 7.996l2.192 2.192a.666.666 0 0 1-.941.942L8.004 8.937 5.811 11.13a.666.666 0 1 1-.941-.942l2.193-2.192L4.87 5.803a.666.666 0 0 1 .941-.941l2.193 2.192 2.193-2.192a.666.666 0 0 1 .47-.195Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgXFill16;
