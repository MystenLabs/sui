// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Menu } from '@headlessui/react';
import { Ooo24 } from '@mysten/icons';
import { Link } from '_src/ui/app/shared/Link';

const AssetsOptionsMenu = () => {
	return (
		<Menu>
			<Menu.Button
				className="cursor-pointer appearance-none border"
				style={{ border: 'none', background: 'none', height: '32px', width: '32px' }}
			>
				<Ooo24 className="text-gray-90 w-full h-full" />
			</Menu.Button>
			<Menu.Items className="absolute top-3 right-0 mt-2 w-50 bg-white divide-y divide-gray-200 rounded-md z-50">
				<div className="rounded-md w-full h-full p-4 shadow-summary-card">
					<Menu.Item>
						{({ active }) => (
							<div className="py-2 hover:bg-sui-light rounded-md">
								<Link
									to="/hidden-assets"
									color="steelDarker"
									weight="medium"
									text="View Hidden Assets"
								/>
							</div>
						)}
					</Menu.Item>
				</div>
			</Menu.Items>
		</Menu>
	);
};

export default AssetsOptionsMenu;
