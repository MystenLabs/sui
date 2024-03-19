// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgViewSmallThumbnails16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M1.333 1.333A.667.667 0 0 0 .667 2v2.667c0 .368.298.666.666.666H4a.667.667 0 0 0 .667-.666V2A.667.667 0 0 0 4 1.333H1.333ZM2 4V2.667h1.333V4H2Z"
			clipRule="evenodd"
		/>
		<path
			fill="currentColor"
			d="M6 3.333c0-.368.298-.666.667-.666h8a.667.667 0 1 1 0 1.333h-8A.667.667 0 0 1 6 3.333ZM6.667 7.333a.667.667 0 1 0 0 1.334h8a.667.667 0 1 0 0-1.334h-8ZM6.667 12a.667.667 0 0 0 0 1.333h8a.667.667 0 1 0 0-1.333h-8Z"
		/>
		<path
			fill="currentColor"
			fillRule="evenodd"
			d="M1.333 10.667a.667.667 0 0 0-.666.666V14c0 .368.298.667.666.667H4A.667.667 0 0 0 4.667 14v-2.667A.667.667 0 0 0 4 10.667H1.333ZM2 13.333V12h1.333v1.333H2ZM.667 6.667c0-.369.298-.667.666-.667H4c.368 0 .667.298.667.667v2.666A.667.667 0 0 1 4 10H1.333a.667.667 0 0 1-.666-.667V6.667ZM2 7.333v1.334h1.333V7.333H2Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgViewSmallThumbnails16;
