// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';

const SvgThumbnailsOnly16 = (props: SVGProps<SVGSVGElement>) => (
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
			d="M1.333 2.667c0-.737.597-1.334 1.334-1.334H6c.736 0 1.333.597 1.333 1.334V6c0 .736-.597 1.333-1.333 1.333H2.667A1.333 1.333 0 0 1 1.333 6V2.667Zm4.667 0H2.667V6H6V2.667Zm2.667 0c0-.737.597-1.334 1.333-1.334h3.333c.737 0 1.334.597 1.334 1.334V6c0 .736-.597 1.333-1.334 1.333H10A1.333 1.333 0 0 1 8.667 6V2.667Zm4.666 0H10V6h3.333V2.667ZM1.333 10c0-.736.597-1.333 1.334-1.333H6c.736 0 1.333.597 1.333 1.333v3.333c0 .737-.597 1.334-1.333 1.334H2.667a1.333 1.333 0 0 1-1.334-1.334V10ZM6 10H2.667v3.333H6V10Zm2.667 0c0-.736.597-1.333 1.333-1.333h3.333c.737 0 1.334.597 1.334 1.333v3.333c0 .737-.597 1.334-1.334 1.334H10a1.333 1.333 0 0 1-1.333-1.334V10Zm4.666 0H10v3.333h3.333V10Z"
			clipRule="evenodd"
		/>
	</svg>
);
export default SvgThumbnailsOnly16;
