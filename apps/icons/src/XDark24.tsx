// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgXDark24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<circle cx={12} cy={12} r={12} fill="currentColor" />
		<path
			fill="#fff"
			d="M16.708 7.292a.999.999 0 0 0-1.413 0l-3.289 3.29-3.289-3.29a.999.999 0 0 0-1.412 1.413l3.289 3.289-3.29 3.289a.998.998 0 1 0 1.413 1.412l3.29-3.289 3.288 3.29a.999.999 0 0 0 1.413-1.413l-3.29-3.29 3.29-3.288a.999.999 0 0 0 0-1.413Z"
		/>
	</svg>
);
export default SvgXDark24;
