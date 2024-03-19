// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgLogoTwitch = (props: SVGProps<SVGSVGElement>) => (
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
			d="m11.458 10.771 2.286-2.286V1.3H2.973v9.472h2.943v1.957l1.957-1.957h3.585ZM1 2.614 1.658 0h13.4v9.143L9.83 14.372H7.2L5.57 16H3.943v-1.628H1V2.614Zm6.857 5.214h-1.3V3.914h1.3v3.914Zm3.6 0h-1.299V3.914h1.3v3.914Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgLogoTwitch;
