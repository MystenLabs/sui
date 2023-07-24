// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgPreview12 = (props: SVGProps<SVGSVGElement>) => (
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
			strokeWidth={1.5}
			d="M4.571 2H3.143C2.512 2 2 2.512 2 3.143v1.143M4.571 10H3.143A1.143 1.143 0 0 1 2 8.857V7.714M7.429 2h1.428C9.488 2 10 2.512 10 3.143v1.143M7.429 10h1.428C9.488 10 10 9.488 10 8.857V7.714"
		/>
	</svg>
);
export default SvgPreview12;
