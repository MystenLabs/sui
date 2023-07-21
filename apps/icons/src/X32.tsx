// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgX32 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 32 32"
		{...props}
	>
		<path
			fill="currentColor"
			d="M8.311 23.409c.43.42 1.133.42 1.543.01l6.006-6.006 6.006 6.006c.42.41 1.123.42 1.543-.01a1.1 1.1 0 0 0 .01-1.533l-6.006-6.016 6.006-6.006c.41-.41.42-1.123-.01-1.533a1.112 1.112 0 0 0-1.543-.01l-6.006 6.006-6.006-6.006a1.112 1.112 0 0 0-1.543 0c-.42.43-.41 1.143 0 1.543l6.006 6.006-6.006 6.016c-.41.4-.42 1.123 0 1.533Z"
		/>
	</svg>
);
export default SvgX32;
