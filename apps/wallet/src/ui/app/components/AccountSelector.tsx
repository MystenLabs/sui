// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Popover, Transition } from '@headlessui/react';
import { ChevronDown12, Copy12 } from '@mysten/icons';

import { useMiddleEllipsis } from '../hooks';
import { useAccounts } from '../hooks/useAccounts';
import { useActiveAddress } from '../hooks/useActiveAddress';
import { useBackgroundClient } from '../hooks/useBackgroundClient';
import { useCopyToClipboard } from '../hooks/useCopyToClipboard';
import { ButtonConnectedTo } from '../shared/ButtonConnectedTo';
import { Text } from '../shared/text';
import { AccountList } from './AccountList';
import { FEATURES } from '_src/shared/experimentation/features';

export function AccountSelector() {
    const allAccounts = useAccounts();
    const activeAddress = useActiveAddress();
    const multiAccountsEnabled = useFeature(FEATURES.WALLET_MULTI_ACCOUNTS).on;
    const activeAddressShort = useMiddleEllipsis(activeAddress);
    const copyToAddress = useCopyToClipboard(activeAddress || '', {
        copySuccessMessage: 'Address copied',
    });
    const backgroundClient = useBackgroundClient();
    if (!allAccounts.length) {
        return null;
    }
    const buttonText = (
        <Text mono variant="bodySmall">
            {activeAddressShort}
        </Text>
    );
    if (!multiAccountsEnabled || allAccounts.length === 1) {
        return (
            <ButtonConnectedTo
                text={buttonText}
                onClick={copyToAddress}
                iconAfter={<Copy12 />}
                bgOnHover="grey"
            />
        );
    }
    return (
        <Popover className="relative z-10">
            {({ close }) => (
                <>
                    <Popover.Button
                        as={ButtonConnectedTo}
                        text={buttonText}
                        iconAfter={<ChevronDown12 />}
                        bgOnHover="grey"
                    />
                    <Transition
                        enter="transition duration-200 ease-out"
                        enterFrom="transform scale-95 opacity-0"
                        enterTo="transform scale-100 opacity-100"
                        leave="transition duration-200 ease-out"
                        leaveFrom="transform scale-100 opacity-100"
                        leaveTo="transform scale-75 opacity-0"
                    >
                        <Popover.Panel className="absolute left-1/2 -translate-x-1/2 w-50 drop-shadow-accountModal mt-2 z-0 rounded-md bg-white">
                            <div className="absolute w-3 h-3 bg-white -top-1 left-1/2 -translate-x-1/2 rotate-45" />
                            <div className="relative px-1.25 my-1.25 max-h-80 overflow-y-auto max-w-full z-10">
                                <AccountList
                                    onAccountSelected={async (
                                        selectedAddress
                                    ) => {
                                        if (selectedAddress !== activeAddress) {
                                            await backgroundClient.selectAccount(
                                                selectedAddress
                                            );
                                        }
                                        close();
                                    }}
                                />
                            </div>
                        </Popover.Panel>
                    </Transition>
                </>
            )}
        </Popover>
    );
}
