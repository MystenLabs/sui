// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef } from 'react';

import { Link, type LinkProps } from '../../shared/Link';

const FooterLink = forwardRef((props: LinkProps & { icon?: React.ReactNode }, forwardedRef) => {
	return (
		<div className="flex gap-1 uppercase bg-none rounded-sm  hover:bg-white/60 p-1 items-center justify-center">
			<Link before={props.icon} weight="semibold" size="captionSmall" {...props} />
		</div>
	);
});

export { FooterLink };
