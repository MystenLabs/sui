// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Add16, Download16 } from '@mysten/icons';
import { Link } from 'react-router-dom';

import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';

const selections = [
    {
        title: 'Yes, let’s create one!',
        desc: 'This creates a new wallet and a 12-word recovery phrase.',
        url: '../create',
        action: 'Create a New Wallet',
        icon: <Add16 className="font-semibold" />,
    },
    {
        title: 'No, I already have one',
        desc: 'Import your existing wallet by entering the 12-word recovery phrase.',
        url: '../import',
        action: 'Import an Existing Wallet',
        icon: <Download16 className="font-semibold" />,
    },
];

const SelectPage = () => {
    return (
        <>
            <Heading variant="heading1" color="gray-90" as="h2" weight="bold">
                New to Sui Wallet?
            </Heading>
            <div className="flex flex-col flex-nowrap gap-7.5 mt-7">
                {selections.map((aSelection) => (
                    <div
                        className={
                            'bg-sui-lightest flex flex-col flex-nowrap items-center gap-3 text-center rounded-15 py-10 px-7.5 max-w-popup-width shadow-wallet-content'
                        }
                        key={aSelection.url}
                    >
                        <Heading
                            variant="heading3"
                            color="gray-90"
                            as="h3"
                            weight="semibold"
                        >
                            {aSelection.title}
                        </Heading>
                        <Text variant="pBody" color="gray-85" weight="medium">
                            {aSelection.desc}
                        </Text>

                        <Link
                            to={aSelection.url}
                            className={
                                'mt-3.5 flex flex-nowrap items-center justify-center bg-hero-dark text-white !rounded-xl py-3.75 px-5 w-full gap-2.5 no-underline font-semibold text-body hover:bg-hero'
                            }
                        >
                            {aSelection.icon}
                            {aSelection.action}
                        </Link>
                    </div>
                ))}
            </div>
        </>
    );
};

export default SelectPage;
