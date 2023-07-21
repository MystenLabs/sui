// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgSpinner16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeWidth={2}
			d="M2.204 6.447A6 6 0 1 0 8 2"
		/>
	</svg>
);
export default SvgSpinner16;
