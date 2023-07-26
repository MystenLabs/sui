// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgTwitchLogo = (props: SVGProps<SVGSVGElement>) => (
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
			fillRule="evenodd"
			d="m16.38 15.81 3.143-3.143v-9.88H4.713V15.81h4.046v2.691l2.691-2.69h4.93ZM2 4.595 2.904 1h18.427v12.571l-7.19 7.19h-3.617L8.287 23H6.047v-2.239H2V4.595Zm9.429 7.168H9.642V6.382h1.787v5.381Zm4.95 0h-1.786V6.382h1.787v5.381Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgTwitchLogo;
