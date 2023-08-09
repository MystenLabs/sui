// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgNftType3D24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M4.75 8 12 4.75 19.25 8 12 11.25 4.75 8ZM4.75 16 12 19.25 19.25 16M19.25 8v8M4.75 8v8M12 11.5V19"
		/>
	</svg>
);
export default SvgNftType3D24;
