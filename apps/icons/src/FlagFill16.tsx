// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgFlagFill16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<g clipPath="url(#flag_fill_16_svg__a)">
			<path
				fill="currentColor"
				d="M8 16c4.377 0 8-3.623 8-8 0-4.369-3.631-8-8.008-8C3.624 0 0 3.631 0 8c0 4.377 3.631 8 8 8Zm-3.075-3.655a.389.389 0 0 1-.384-.376V5.184c0-.345.173-.596.502-.745.29-.141.541-.212 1.177-.212 1.45 0 2.368.722 3.74.722.667 0 1.02-.173 1.256-.173.33 0 .455.173.455.416V9.17c0 .368-.157.604-.494.768-.306.134-.557.196-1.177.196-1.396 0-2.306-.706-3.741-.706-.502 0-.816.095-.973.157v2.385c0 .211-.14.376-.36.376Z"
			/>
		</g>
		<defs>
			<clipPath id="flag_fill_16_svg__a">
				<path fill="#fff" d="M0 0h16v16H0z" />
			</clipPath>
		</defs>
	</svg>
);
export default SvgFlagFill16;
