// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgArrowBgFill16 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 16 16"
		{...props}
	>
		<rect width={16} height={16} fill="currentColor" rx={2} />
		<path
			fill="#fff"
			d="M10.285 7.571a.5.5 0 0 1 0 .858l-3.528 2.117a.5.5 0 0 1-.757-.43V5.884a.5.5 0 0 1 .757-.429l3.528 2.117Z"
		/>
	</svg>
);
export default SvgArrowBgFill16;
