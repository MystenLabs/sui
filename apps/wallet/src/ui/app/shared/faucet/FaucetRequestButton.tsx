// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { FaucetRateLimitError, getFaucetHost } from '@mysten/sui.js/faucet';
import { toast } from 'react-hot-toast';

import FaucetMessageInfo from './FaucetMessageInfo';
import { useFaucetMutation } from './useFaucetMutation';
import { useFaucetRateLimiter } from './useFaucetRateLimiter';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import { Button, type ButtonProps } from '_app/shared/ButtonUI';
import { useAppSelector } from '_hooks';
import { API_ENV } from '_src/shared/api-env';

export type FaucetRequestButtonProps = {
	variant?: ButtonProps['variant'];
	size?: ButtonProps['size'];
};

const FAUCET_HOSTS = {
	[API_ENV.local]: getFaucetHost('localnet'),
	[API_ENV.devNet]: getFaucetHost('devnet'),
	[API_ENV.testNet]: getFaucetHost('testnet'),
};

function FaucetRequestButton({ variant = 'primary', size = 'narrow' }: FaucetRequestButtonProps) {
	const network = useAppSelector(({ app }) => app.apiEnv);
	const networkName = API_ENV_TO_INFO[network].name.replace(/sui\s*/gi, '');
	const [isRateLimited, rateLimit] = useFaucetRateLimiter();

	const mutation = useFaucetMutation({
		host: network in FAUCET_HOSTS ? FAUCET_HOSTS[network as keyof typeof FAUCET_HOSTS] : null,
		onError: (error) => {
			if (error instanceof FaucetRateLimitError) {
				rateLimit();
			}
		},
	});

	return mutation.enabled ? (
		<Button
			data-testid="faucet-request-button"
			variant={variant}
			size={size}
			disabled={isRateLimited}
			onClick={() => {
				toast.promise(mutation.mutateAsync(), {
					loading: <FaucetMessageInfo loading />,
					success: (totalReceived) => <FaucetMessageInfo totalReceived={totalReceived} />,
					error: (error) => <FaucetMessageInfo error={error.message} />,
				});
			}}
			loading={mutation.isMutating}
			text={`Request ${networkName} SUI Tokens`}
		/>
	) : null;
}

export default FaucetRequestButton;
