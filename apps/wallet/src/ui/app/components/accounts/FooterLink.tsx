// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link, type LinkProps } from '../../shared/Link';

export function FooterLink({ icon, ...props }: LinkProps & { icon?: React.ReactNode }) {
	return (
		<div className="flex gap-1 uppercase bg-none rounded-sm  hover:bg-white/60 p-1 items-center justify-center">
			<Link before={icon} weight="semibold" size="captionSmall" {...props} />
		</div>
	);
}
