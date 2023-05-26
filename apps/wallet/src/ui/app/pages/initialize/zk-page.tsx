import { useGetSystemState } from '@mysten/core';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'react-hot-toast';

import Alert from '../../components/alert';
import NetworkSelector from '../../components/network-selector';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { CardLayout } from '../../shared/card-layout';

export function ZkPage() {
    const backgroundClient = useBackgroundClient();
    const { data, isLoading, error, refetch } = useGetSystemState();
    const currentEpoch = data ? Number(data.epoch) : null;
    const createAccountMutation = useMutation({
        mutationKey: ['create', 'zk', 'account'],
        mutationFn: async () => {
            if (currentEpoch === null) {
                throw new Error('Missing current epoch');
            }
            await backgroundClient.createZkAccount(currentEpoch);
        },
        onError: (e) =>
            toast.error((e as Error)?.message || 'Error creating account'),
        onSuccess: () => toast.success('ZK account created'),
    });
    return (
        <CardLayout title="Create a new ZK Account">
            <div className="flex flex-col flex-1 justify-center gap-3">
                {error ? (
                    <Alert>
                        <div>Error loading current epoch</div>
                        <div>
                            <Button
                                text="Retry"
                                onClick={() => refetch()}
                                loading={isLoading}
                                variant="plain"
                            />
                        </div>
                    </Alert>
                ) : null}
                <NetworkSelector />
                <Button
                    text="Login with google"
                    loading={createAccountMutation.isLoading || isLoading}
                    disabled={
                        !!(
                            createAccountMutation.isSuccess ||
                            isLoading ||
                            error ||
                            !currentEpoch
                        )
                    }
                    onClick={() => createAccountMutation.mutate()}
                />
            </div>
        </CardLayout>
    );
}
