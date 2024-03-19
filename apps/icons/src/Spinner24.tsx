// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgSpinner24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path
			stroke="#758F9E"
			strokeLinecap="round"
			strokeWidth={1.75}
			d="M4.273 9.93A8 8 0 1 0 12 4"
		/>
	</svg>
);
export default SvgSpinner24;
