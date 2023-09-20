// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgLogoLedger = (props: SVGProps<SVGSVGElement>) => (
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
			d="M0 10.976v3.956h6.018v-.877H.877v-3.079H0Zm15.123 0v3.079H9.982v.877H16v-3.956h-.877Zm-9.096-6.02v6.02h3.955v-.791H6.904v-5.23h-.877ZM0 1v3.956h.877V1.877h5.141V1H0Zm9.982 0v.877h5.141v3.079H16V1H9.982Z"
		/>
	</svg>
);
export default SvgLogoLedger;
