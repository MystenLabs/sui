// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate, useParams } from 'react-router-dom';
import { useEffect } from 'react';
import { KioskItems } from '../components/Kiosk/KioskItems';

export default function SingleKiosk() {
	const { kioskId } = useParams();
	const navigate = useNavigate();

	useEffect(() => {
		if (kioskId) return;
		navigate('/');
	}, [navigate, kioskId]);

	return (
		<div className="container">
			<KioskItems kioskId={kioskId}></KioskItems>
		</div>
	);
}
