// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgSender16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<g clipPath="url(#sender_16_svg__a)">
			<circle cx={8} cy={8} r={8} fill="#EBECED" />
			<circle cx={8} cy={8} r={3} fill="#A0B6C3" />
		</g>
		<defs>
			<clipPath id="sender_16_svg__a">
				<path fill="#fff" d="M0 0h16v16H0z" />
			</clipPath>
		</defs>
	</svg>
);
export default SvgSender16;
