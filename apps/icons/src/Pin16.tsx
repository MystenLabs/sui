// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgPin16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<g clipPath="url(#pin_16_svg__a)">
			<path
				fill="currentColor"
				d="M2.729 16C.913 16 0 15.105 0 13.306V2.694C0 .895.913 0 2.729 0H13.27C15.096 0 16 .904 16 2.694v10.612c0 1.79-.904 2.694-2.729 2.694H2.729Zm1.756-6.327c0 .4.26.643.66.643h2.312v1.6c0 .973.39 1.78.556 1.78.165 0 .556-.807.556-1.78v-1.6h2.312c.409 0 .66-.243.66-.643 0-.947-.764-1.973-2.042-2.425l-.148-2.103c.652-.374 1.191-.79 1.434-1.095.113-.165.183-.322.183-.47 0-.277-.217-.477-.539-.477H5.597c-.322 0-.539.2-.539.478 0 .147.07.313.2.478.243.304.774.721 1.417 1.086l-.14 2.103c-1.286.452-2.05 1.478-2.05 2.425Z"
			/>
		</g>
		<defs>
			<clipPath id="pin_16_svg__a">
				<path fill="#fff" d="M0 0h16v16H0z" />
			</clipPath>
		</defs>
	</svg>
);
export default SvgPin16;
