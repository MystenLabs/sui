// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgAdd16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M9 2a1 1 0 0 0-2 0v5H2a1 1 0 0 0 0 2h5v5a1 1 0 1 0 2 0V9h5a1 1 0 1 0 0-2H9V2Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgAdd16;
