// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';

import { Button } from '../Base/Button';

export function KioskNotFound() {
	const navigate = useNavigate();

	return (
		<div className="min-h-[70vh] flex items-center justify-center gap-4 mt-6 text-center">
			<div>
				<h2 className="font-bold text-2xl mb-2">No kiosk found</h2>
				<p>There is no kiosk with the id you have entered. Confirm the object id and try again.</p>
				<Button onClick={() => navigate('/')} className="mt-8 bg-primary text-white">
					Open your kiosk
				</Button>
			</div>
		</div>
	);
}
