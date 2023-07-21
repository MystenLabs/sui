// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgCode16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M2.75 3.655c0-.5.405-.905.905-.905h8.69c.5 0 .905.405.905.905v8.69c0 .5-.405.905-.905.905h-8.69a.905.905 0 0 1-.905-.905v-8.69Zm.905-2.405A2.405 2.405 0 0 0 1.25 3.655v8.69a2.405 2.405 0 0 0 2.405 2.405h8.69a2.405 2.405 0 0 0 2.405-2.405v-8.69a2.405 2.405 0 0 0-2.405-2.405h-8.69Zm2.157 5.158A.75.75 0 1 0 4.81 7.523l1.45 1.305-1.45 1.304a.75.75 0 0 0 1.003 1.115l2.07-1.862a.75.75 0 0 0 0-1.115l-2.07-1.862Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgCode16;
