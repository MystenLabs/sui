// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgFacebookLogo = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 21 22"
		{...props}
	>
		<path
			fill="#fff"
			d="M20.833 11.063C20.833 5.356 16.207.729 10.5.729S.167 5.356.167 11.063c0 5.157 3.778 9.432 8.718 10.208V14.05H6.262v-2.987h2.623V8.786c0-2.59 1.543-4.02 3.904-4.02 1.13 0 2.313.202 2.313.202V7.51h-1.303c-1.284 0-1.684.796-1.684 1.613v1.939h2.865l-.458 2.987h-2.407v7.22c4.94-.775 8.718-5.05 8.718-10.207Z"
		/>
	</svg>
);
export default SvgFacebookLogo;
