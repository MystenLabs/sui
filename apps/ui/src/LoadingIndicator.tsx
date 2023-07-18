// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Spinner16 } from '@mysten/icons';

export interface LoadingIndicatorProps {
	text?: string;
}

export function LoadingIndicator({ text }: LoadingIndicatorProps) {
	return (
		<div className="inline-flex flex-row flex-nowrap items-center gap-3">
			<Spinner16 className="animate-spin text-steel" />
			{text ? <div className="text-body font-medium text-steel-dark">{text}</div> : null}
		</div>
	);
}
