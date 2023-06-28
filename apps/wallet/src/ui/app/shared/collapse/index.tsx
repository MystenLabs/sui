// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { ChevronRight12 } from '@mysten/icons';
import cl from 'classnames';

import { Text } from '_app/shared/text';

import type { ReactNode } from 'react';

type CollapseProps = {
	title: string;
	initialIsOpen?: boolean;
	children: ReactNode | ReactNode[];
};

export function Collapse({ title, children, initialIsOpen = false }: CollapseProps) {
	return (
		<div>
			<Disclosure defaultOpen={initialIsOpen}>
				{({ open }) => (
					<>
						<Disclosure.Button as="div" className="flex w-full flex-col gap-2 cursor-pointer">
							<div className="flex items-center gap-1">
								<Text nowrap variant="caption" weight="semibold" color="steel-darker">
									{title}
								</Text>
								<div className="h-px bg-gray-45 w-full" />
								<ChevronRight12 className={cl('h-3 w-3 text-gray-45', open && 'rotate-90')} />
							</div>
						</Disclosure.Button>

						<Disclosure.Panel>
							<div className="pt-3">{children}</div>
						</Disclosure.Panel>
					</>
				)}
			</Disclosure>
		</div>
	);
}
