// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';
import { ImportPrivateKeyForm } from '../../components/accounts/ImportPrivateKeyForm';
import { Heading } from '../../shared/heading';
import { Text } from '_app/shared/text';

export function ImportPrivateKeyPage() {
	const navigate = useNavigate();

	return (
		<div className="rounded-20 bg-sui-lightest shadow-wallet-content flex flex-col items-center px-6 py-10 w-full h-full">
			<Text variant="caption" color="steel-dark" weight="semibold">
				Wallet Setup
			</Text>
			<div className="text-center mt-2.5">
				<Heading variant="heading1" color="gray-90" as="h1" weight="bold">
					Import Private Key
				</Heading>
			</div>
			<div className="mt-6 w-full grow">
				<ImportPrivateKeyForm
					onSubmit={(formValues) => {
						// eslint-disable-next-line no-console
						console.log(
							'TODO: Do something when the user submits the form successfully',
							formValues,
						);
						navigate('/accounts/protect-account');
					}}
				/>
			</div>
		</div>
	);
}
