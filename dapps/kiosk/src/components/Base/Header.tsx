// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useLocation, useNavigate } from 'react-router-dom';
import FindKiosk from '../Kiosk/FindKiosk';
import { SuiConnectButton } from './SuiConnectButton';
import { Button } from './Button';
import classNames from 'classnames';

export function Header() {
	const navigate = useNavigate();

	const location = useLocation();
	const isHome = location.pathname === '/';

	return (
		<div className="border-b border-gray-400">
			<div className="md:flex items-center gap-2 container py-4 ">
				<button
					className="text-lg font-bold text-center mr-3 bg-transparent ease-in-out duration-300 rounded border border-transparent py-2 px-4 bg-gray-200"
					onClick={() => navigate('/')}
				>
					Kiosk demo
				</button>
				<Button
					className={classNames(
						!isHome && '!bg-gray-100',
						'mr-2 bg-transparent ease-in-out duration-300 rounded border border-transparent py-2 px-4',
					)}
					disabled={isHome}
					onClick={() => navigate('/')}
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						width="22"
						height="22"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						strokeWidth="1"
						strokeLinecap="round"
						strokeLinejoin="round"
					>
						<path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path>
						<polyline points="9 22 9 12 15 12 15 22"></polyline>
					</svg>
				</Button>
				<FindKiosk />
				<div className="ml-auto my-3 md:my-1">
					<SuiConnectButton></SuiConnectButton>
				</div>
			</div>
		</div>
	);
}
