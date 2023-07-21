// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgLogout24 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path
			fill="currentColor"
			fillRule="evenodd"
			d="M9.25 11.25a.75.75 0 0 0 0 1.5h7.59l-2.1 1.95a.75.75 0 1 0 1.02 1.1l3.499-3.249a.748.748 0 0 0 0-1.102L15.76 8.2a.75.75 0 1 0-1.02 1.1l2.1 1.95H9.25Z"
			clipRule="evenodd"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M9.5 4.75H8a3 3 0 0 0-3 3v8.5a3 3 0 0 0 3 3h1.5"
		/>
	</svg>
);
export default SvgLogout24;
