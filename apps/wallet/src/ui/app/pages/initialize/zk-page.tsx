import { Popover } from '@headlessui/react';
import { useGetSystemState } from '@mysten/core';
import { Check12, X12 } from '@mysten/icons';
import { formatAddress } from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'react-hot-toast';
import * as Yup from 'yup';

import Alert from '../../components/alert';
import Logo from '../../components/logo';
import NetworkSelector from '../../components/network-selector';
import { useAppSelector } from '../../hooks';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Link } from '../../shared/Link';
import { ModalDialog } from '../../shared/ModalDialog';
import { CardLayout } from '../../shared/card-layout';
import { Heading } from '../../shared/heading';
import { Text } from '../../shared/text';
import { Tooltip } from '../../shared/tooltip';
import GoogleIcon from '_assets/images/google.svg';
import Twitch from '_assets/images/twitch.svg';

const pinValidation = Yup.string().matches(/^\d+$/);

export function ZkPage() {
    const networkName = useAppSelector(({ app: { apiEnv } }) => apiEnv);
    const backgroundClient = useBackgroundClient();
    const { data, isLoading, error, refetch } = useGetSystemState();
    const currentEpoch = data ? Number(data.epoch) : null;
    const createAccountMutation = useMutation({
        mutationKey: ['create', 'zk', 'account'],
        mutationFn: ({ accountPin }: { accountPin?: string }) => {
            if (currentEpoch === null) {
                throw new Error('Missing current epoch');
            }
            return backgroundClient.createZkAccount(currentEpoch, accountPin);
        },
        onError: (e) =>
            toast.error((e as Error)?.message || 'Error creating account'),
        onSuccess: ({ address, email, pin }) =>
            toast.success(
                (t) => (
                    <div className="flex flex-col gap-1 relative">
                        <X12
                            className="absolute top-0 right-0 cursor-pointer"
                            onClick={() => toast.dismiss(t.id)}
                        />
                        <div className="mb-1">
                            <Heading variant="heading6">
                                Welcome aboard!
                            </Heading>
                        </div>
                        <Text>
                            Account{' '}
                            <span className="font-semibold font-mono">
                                {formatAddress(address)}
                            </span>{' '}
                            was added to your wallet.
                        </Text>
                        <Text variant="bodySmall">
                            To recover your account use your google account{' '}
                            <span className="font-semibold">{email}</span> and
                            the following pin{' '}
                            <span className="font-semibold font-mono">
                                {pin}
                            </span>
                        </Text>
                    </div>
                ),
                { duration: Infinity }
            ),
    });
    const [isPinInputVisible, setIsPinInputVisible] = useState(false);
    const [isAdvanced, setIsAdvanced] = useState(false);
    const [pin, setPin] = useState('');
    return (
        <CardLayout>
            <div className="flex flex-col flex-1 items-center gap-4 self-stretch">
                <Popover className="relative self-stretch flex justify-center">
                    <Popover.Button as="div">
                        <Logo networkName={networkName} />
                    </Popover.Button>
                    <Popover.Panel className="absolute z-10 top-[100%] shadow-lg">
                        <NetworkSelector />
                    </Popover.Panel>
                </Popover>
                <div className="text-center flex flex-col gap-1">
                    <Heading
                        variant="heading4"
                        weight="semibold"
                        color="gray-90"
                    >
                        Welcome to Sui Wallet
                    </Heading>
                    <Text variant="pBody" weight="medium" color="steel-dark">
                        Connecting you to the decentralized web and Sui network.
                    </Text>
                </div>
                <div className="flex-1" />
                <Button
                    variant="outline"
                    text="Sign In with Google"
                    size="tall"
                    loading={createAccountMutation.isLoading || isLoading}
                    disabled={
                        !!(
                            createAccountMutation.isSuccess ||
                            isLoading ||
                            error ||
                            currentEpoch === null ||
                            isPinInputVisible
                        )
                    }
                    onClick={() => {
                        if (isAdvanced) {
                            setIsPinInputVisible(true);
                        } else {
                            createAccountMutation.mutate({});
                        }
                    }}
                    before={<GoogleIcon />}
                />
                <Button
                    variant="twitch"
                    text="Sign In with Twitch"
                    size="tall"
                    disabled
                    before={<Twitch />}
                />
                <div className="flex">
                    <label className="flex items-center justify-center h-5 mb-0 mr-5 text-sui-dark gap-1.25 relative cursor-pointer">
                        <input
                            type="checkbox"
                            name="agree"
                            id="agree"
                            className="peer/agree invisible ml-2"
                            disabled={
                                !!isPinInputVisible ||
                                createAccountMutation.isLoading
                            }
                            onChange={() => setIsAdvanced((value) => !value)}
                        />
                        <span className="absolute top-0 left-0 h-5 w-5 bg-white peer-checked/agree:bg-success peer-checked/agree:shadow-none border-gray-50 border rounded shadow-button flex justify-center items-center">
                            <Check12 className="text-white text-body font-semibold" />
                        </span>
                        <Tooltip tip="Use advance mode to use a specific pin">
                            <Text
                                variant="bodySmall"
                                color="steel-dark"
                                weight="normal"
                            >
                                Advanced
                            </Text>
                        </Tooltip>
                    </label>
                </div>
                <div className="flex-1" />
                {error ? (
                    <div className="self-stretch">
                        <Alert>
                            Error loading current epoch.{' '}
                            <div className="inline-block">
                                <Link
                                    color="suiDark"
                                    text="Retry"
                                    onClick={() => refetch()}
                                    loading={isLoading}
                                />
                            </div>
                        </Alert>
                    </div>
                ) : null}
            </div>
            <ModalDialog
                isOpen={isPinInputVisible}
                onClose={() => setIsPinInputVisible(false)}
                title="Insert your pin"
                body={
                    <div className="flex flex-col gap-2 mt-3">
                        <input
                            type="string"
                            onChange={({ target: { value } }) => {
                                try {
                                    pinValidation.validateSync(value);
                                    setPin(value);
                                } catch (e) {
                                    setPin(value.trim() === '' ? '' : pin);
                                }
                            }}
                            value={pin}
                            className={
                                'peer h-11 w-full text-body text-steel-dark font-medium flex items-center gap-5 bg-white py-2.5 pr-0 pl-3 border border-solid  border-gray-45 rounded-2lg shadow-button focus:border-steel focus:shadow-none placeholder-gray-65'
                            }
                            placeholder="Only numbers"
                        />
                    </div>
                }
                footer={
                    <div className="w-full flex flex-row self-center gap-3">
                        <Button
                            variant="outline"
                            size="tall"
                            text="Cancel"
                            onClick={() => setIsPinInputVisible(false)}
                        />
                        <Button
                            variant="outline"
                            size="tall"
                            text="Continue"
                            disabled={
                                currentEpoch === null ||
                                createAccountMutation.isLoading ||
                                !pin.length
                            }
                            onClick={() => {
                                createAccountMutation
                                    .mutateAsync({ accountPin: pin })
                                    .then(() => setIsPinInputVisible(false));
                            }}
                            loading={createAccountMutation.isLoading}
                        />
                    </div>
                }
            />
        </CardLayout>
    );
}
