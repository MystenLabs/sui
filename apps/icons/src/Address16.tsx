// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgAddress16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<g clipPath="url(#address_16_svg__a)">
			<path
				fill="currentColor"
				fillRule="evenodd"
				d="M8.447.152a1 1 0 0 0-.895 0L.886 3.485a1 1 0 0 0 0 1.789l6.666 3.333a1 1 0 0 0 .895 0l6.667-3.333a1 1 0 0 0 0-1.789L8.447.152ZM.439 7.599a1 1 0 0 1 1.341-.447L8 10.262l6.22-3.11a1 1 0 1 1 .893 1.789l-6.666 3.333a1 1 0 0 1-.895 0L.886 8.94a1 1 0 0 1-.447-1.342Zm1.341 3.127a1 1 0 1 0-.894 1.789l6.666 3.333a1 1 0 0 0 .895 0l6.667-3.333a1 1 0 1 0-.895-1.789L8 13.836l-6.219-3.11Z"
				clipRule="evenodd"
			/>
		</g>
		<defs>
			<clipPath id="address_16_svg__a">
				<path fill="#fff" d="M0 0h16v16H0z" />
			</clipPath>
		</defs>
	</svg>
);
export default SvgAddress16;
