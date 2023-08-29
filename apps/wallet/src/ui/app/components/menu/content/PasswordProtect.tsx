// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useNavigate } from 'react-router-dom';
import { ProtectAccountForm } from '../../accounts/ProtectAccountForm';
import Overlay from '../../overlay';
import { useNextMenuUrl } from '_components/menu/hooks';

export function PasswordProtect() {
	const mainMenuUrl = useNextMenuUrl(true, '/');
	const navigate = useNavigate();
	return (
		<Overlay
			showModal={true}
			title={'Password Protect Accounts'}
			closeOverlay={() => navigate(mainMenuUrl)}
		>
			<div className="flex flex-col w-full mt-2.5">
				<ProtectAccountForm
					displayToS={false}
					submitButtonText="Save"
					onSubmit={(formValues) => {
						// eslint-disable-next-line no-console
						console.log(
							'TODO: Do something when the user submits the form successfully',
							formValues,
						);
					}}
				/>
			</div>
		</Overlay>
	);
}
