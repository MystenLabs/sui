// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgArrowSortUp12 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 12 12"
		{...props}
	>
		<path
			fill="currentColor"
			d="M6.433 10.75a.5.5 0 0 1-.866 0l-1.732-3A.5.5 0 0 1 4.268 7h3.464a.5.5 0 0 1 .433.75l-1.732 3Z"
			opacity={0.2}
		/>
		<path
			fill="currentColor"
			d="M5.567 1.25a.5.5 0 0 1 .866 0l1.732 3a.5.5 0 0 1-.433.75H4.268a.5.5 0 0 1-.433-.75l1.732-3Z"
		/>
	</svg>
);
export default SvgArrowSortUp12;
