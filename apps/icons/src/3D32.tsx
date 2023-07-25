// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const Svg3D32 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M6.333 10.667 16 6.333l9.667 4.334L16 15l-9.667-4.333ZM6.333 21.333 16 25.667l9.667-4.334M25.667 10.667v10.666M6.333 10.667v10.666M16 15.333v10"
		/>
	</svg>
);
export default Svg3D32;
