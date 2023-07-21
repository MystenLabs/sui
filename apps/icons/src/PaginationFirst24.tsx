// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgPaginationFirst24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path
			fill="#fff"
			stroke="currentColor"
			d="m18.521 6.807-7.912 4.315a1 1 0 0 0 0 1.756l7.912 4.315A1 1 0 0 0 20 16.315v-8.63a1 1 0 0 0-1.479-.878Z"
		/>
		<path
			fill="#fff"
			stroke="currentColor"
			d="M10.521 6.807 2.61 11.122a1 1 0 0 0 0 1.756l7.912 4.315A1 1 0 0 0 12 16.315v-8.63a1 1 0 0 0-1.479-.878Z"
		/>
	</svg>
);
export default SvgPaginationFirst24;
