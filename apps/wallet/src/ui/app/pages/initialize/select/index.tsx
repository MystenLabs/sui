// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Link } from 'react-router-dom';

import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import Icon, { SuiIcons } from '_components/icon';

import st from './Select.module.scss';

const selections = [
    {
        title: 'Yes, let’s create one!',
        desc: 'This creates a new wallet and a 12-word recovery phrase.',
        url: '../create',
        action: 'Create a New Wallet',
        icon: SuiIcons.Plus,
    },
    {
        title: 'No, I already have one',
        desc: 'Import your existing wallet by entering the 12-word recovery phrase.',
        url: '../import',
        action: 'Import an Existing Wallet',
        icon: SuiIcons.Download,
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
                            'bg-alice-blue flex flex-col flex-nowrap items-center gap-3 text-center rounded-15 py-10 px-7.5 max-w-popup-width shadow-wallet-content'
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
                        <Text variant="p1" color="gray-85" weight="medium">
                            {aSelection.desc}
                        </Text>

                        <Link
                            to={aSelection.url}
                            className={cl('btn mt-7.5', st.action)}
                        >
                            <Icon
                                icon={aSelection.icon}
                                className={cl(st.icon, 'font-normal')}
                            />
                            {aSelection.action}
                        </Link>
                    </div>
                ))}
            </div>
        </>
    );
};

export default SelectPage;
