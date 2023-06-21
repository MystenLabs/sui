// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ReactComponent as SpinnerSvg } from './icons/spinner.svg';

export interface LoadingSpinnerProps {
	text?: string;
}

export function LoadingSpinner({ text }: LoadingSpinnerProps) {
	return (
		<div className="inline-flex flex-row flex-nowrap items-center gap-3">
			<SpinnerSvg className="animate-spin text-steel" />
			{text ? <div className="text-body font-medium text-steel-dark">{text}</div> : null}
		</div>
	);
}
