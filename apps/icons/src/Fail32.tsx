// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { SVGProps } from 'react';
const SvgFail32 = (props: SVGProps<SVGSVGElement>) => (
	<svg
		xmlns="http://www.w3.org/2000/svg"
		width="1em"
		height="1em"
		fill="none"
		viewBox="0 0 32 32"
		{...props}
	>
		<path
			fill="currentColor"
			d="M16 26.037c5.518 0 10.088-4.56 10.088-10.088 0-5.517-4.57-10.088-10.098-10.088-5.517 0-10.078 4.57-10.078 10.088 0 5.528 4.57 10.088 10.088 10.088Zm-3.457-5.644c-.547 0-.938-.43-.938-.957 0-.254.118-.528.362-.762l6.719-6.797c.224-.244.478-.361.752-.361.527 0 .957.42.957.957 0 .254-.127.527-.362.752l-6.719 6.806c-.234.235-.468.362-.771.362Z"
		/>
	</svg>
);
export default SvgFail32;
