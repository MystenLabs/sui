// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Menu } from '@headlessui/react';
import { Ooo24 } from '@mysten/icons';
import { Link } from 'react-router-dom';

const AssetsOptionsMenu = () => {
	return (
		<Menu>
			<Menu.Button
				style={{
					border: 'none',
					background: 'none',
					height: '32px',
					width: '32px',
					cursor: 'pointer',
				}}
			>
				<Ooo24 className="text-gray-90 w-full h-full" />
			</Menu.Button>
			<Menu.Items className="absolute top-4 right-0 mt-2 w-50 bg-white divide-y divide-gray-200 rounded-md z-50">
				<div className="rounded-md w-full h-full p-2 shadow-summary-card">
					<Menu.Item>
						{({ active }) => (
							<div className="p-3 hover:bg-sui-light bg-opacity-50 rounded-md">
								<Link
									to="/nfts/hidden-assets"
									className="no-underline text-steel-darker hover:text-steel-darker focus:text-steel-darker disabled:text-steel-darker font-medium text-bodySmall"
								>
									View Hidden Assets
								</Link>
							</div>
						)}
					</Menu.Item>
				</div>
			</Menu.Items>
		</Menu>
	);
};

export default AssetsOptionsMenu;
