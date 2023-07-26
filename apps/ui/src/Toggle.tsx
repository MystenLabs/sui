// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Root, type SwitchProps, Thumb } from '@radix-ui/react-switch';

export function Toggle(props: Omit<SwitchProps, 'className'>) {
	return (
		<Root
			className="relative h-3.75 w-[26px] rounded-full bg-gray-60/70 transition-colors data-[state=checked]:bg-success"
			{...props}
		>
			<Thumb className="block h-[11px] w-[11px] translate-x-0.5 rounded-full bg-white transition-transform will-change-transform data-[state=checked]:translate-x-[13px]" />
		</Root>
	);
}
