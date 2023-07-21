// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgMediaPlay16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<path
			fill="#fff"
			d="M16 7.996C16 12.388 12.394 16 8 16c-4.386 0-8-3.612-8-8.004C0 3.612 3.606 0 8 0c4.386 0 8 3.612 8 7.996Z"
		/>
		<path
			fill="currentColor"
			d="M8 16c4.394 0 8-3.612 8-8.004C16 3.612 12.386 0 8 0 3.606 0 0 3.612 0 7.996 0 12.388 3.614 16 8 16Zm-1.53-4.816c-.38.22-.834.015-.834-.386V5.21c0-.402.47-.606.834-.394l4.682 2.733c.333.205.34.712 0 .917L6.47 11.184Z"
		/>
	</svg>
);
export default SvgMediaPlay16;
