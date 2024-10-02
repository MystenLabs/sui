// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { normalizeSuiAddress } from '@mysten/sui/utils';
import classnames from 'clsx';
import { FormEvent, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

export default function FindKiosk() {
	const { id } = useParams();

	const [searchKiosk, setSearchKioskId] = useState<string>(id || '');
	const navigate = useNavigate();

	const viewKiosk = (e?: FormEvent<HTMLFormElement>) => {
		if (!searchKiosk || viewingSearchKiosk) return;
		e?.preventDefault();

		const id = normalizeSuiAddress(searchKiosk);
		navigate(`/kiosk/${id}`);
		setSearchKioskId(id);
	};

	const viewingSearchKiosk = searchKiosk === id;
	const isObjectIdInput = (val: string | undefined) => val?.length === 66 || val?.length === 64;

	const onInput = (e: any) => {
		setSearchKioskId(e.target.value);
	};

	const canSearch = !(id === searchKiosk || !isObjectIdInput(searchKiosk));

	return (
		<form onSubmit={viewKiosk} className="text-center lg:min-w-[700px]">
			<div className="flex items-center bg-gray-100 border rounded border-gray-300 overflow-hidden">
				<div className="basis-10/12">
					<input
						type="text"
						id="search"
						role="search"
						value={searchKiosk}
						onInput={onInput}
						className="bg-gray-100 border lg:min-w-[600px] text-gray-900 placeholder:text-gray-500 text-sm rounded rounded-r-none
             focus:ring-transparent
            focus:border-primary block w-full p-2.5 outline-primary"
						placeholder="Enter an address or a Sui Kiosk ID to search for a kiosk..."
						required
					/>
				</div>
				<button
					type="submit"
					className={classnames(
						'basis-2/12 w-full h-[42px] text-primary text-xs mx-auto disabled:opacity-60 !rounded-l-none',
						canSearch && 'bg-primary !text-white',
					)}
					disabled={!canSearch}
				>
					Search
				</button>
			</div>
		</form>
	);
}
