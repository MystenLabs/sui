// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgMicrosoftLogo = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 24 24"
		{...props}
	>
		<path fill="#F25022" d="M11.474 2H2v9.474h9.474V2Z" />
		<path fill="#00A4EF" d="M11.474 12.526H2V22h9.474v-9.474Z" />
		<path fill="#7FBA00" d="M22 2h-9.474v9.474H22V2Z" />
		<path fill="#FFB900" d="M22 12.526h-9.474V22H22v-9.474Z" />
	</svg>
);
export default SvgMicrosoftLogo;
