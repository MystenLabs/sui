// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgWallet32 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 32 32"
		{...props}
	>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M25.667 11v12A2.667 2.667 0 0 1 23 25.667H9A2.667 2.667 0 0 1 6.333 23V9"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			d="M22 17.333a.667.667 0 1 1-1.334 0 .667.667 0 0 1 1.334 0Z"
		/>
		<path
			stroke="currentColor"
			strokeLinecap="round"
			strokeLinejoin="round"
			strokeWidth={1.5}
			d="M23 11h2.667M23 11H8.667a2.333 2.333 0 0 1 0-4.667h11.666A2.667 2.667 0 0 1 23 9v2Z"
		/>
	</svg>
);
export default SvgWallet32;
