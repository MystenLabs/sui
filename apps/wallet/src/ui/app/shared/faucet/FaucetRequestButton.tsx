// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { FaucetRateLimitError } from '@mysten/sui.js';
import { toast } from 'react-hot-toast';

import FaucetMessageInfo from './FaucetMessageInfo';
import { useFaucetMutation } from './useFaucetMutation';
import { useFaucetRateLimiter } from './useFaucetRateLimiter';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import { Button, type ButtonProps } from '_app/shared/ButtonUI';
import { useAppSelector } from '_hooks';

export type FaucetRequestButtonProps = {
    variant?: ButtonProps['variant'];
};

function FaucetRequestButton({
    variant = 'primary',
}: FaucetRequestButtonProps) {
    const network = useAppSelector(({ app }) => app.apiEnv);
    const networkName = API_ENV_TO_INFO[network].name.replace(/sui\s*/gi, '');
    const [isRateLimited, rateLimit] = useFaucetRateLimiter();
    const mutation = useFaucetMutation({
        onError: (error) => {
            if (error instanceof FaucetRateLimitError) {
                rateLimit();
            }
        },
    });

    return mutation.enabled ? (
        <Button
            variant={variant}
            disabled={isRateLimited}
            onClick={() => {
                toast.promise(mutation.mutateAsync(), {
                    loading: <FaucetMessageInfo loading />,
                    success: (totalReceived) => (
                        <FaucetMessageInfo totalReceived={totalReceived} />
                    ),
                    error: (error) => (
                        <FaucetMessageInfo error={error.message} />
                    ),
                });
            }}
            loading={mutation.isMutating}
            text={`Request ${networkName} SUI Tokens`}
        />
    ) : null;
}

export default FaucetRequestButton;
