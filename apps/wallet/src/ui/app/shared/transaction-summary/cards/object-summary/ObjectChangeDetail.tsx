// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import { type WithDisplayFields } from '@mysten/core';
import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import {
    type SuiObjectChange,
    SuiObjectChangePublished,
    SuiObjectChangeTransferred,
    formatAddress,
    is,
} from '@mysten/sui.js';

import { Text } from '../../../text';
import { ObjectChangeDisplay } from './ObjectChangeDisplay';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';

export function ObjectDetail({
    change,
}: {
    change: WithDisplayFields<SuiObjectChange>;
    ownerKey: string;
}) {
    if (
        is(change, SuiObjectChangeTransferred) ||
        is(change, SuiObjectChangePublished)
    ) {
        return null;
    }
    const [packageId, moduleName, typeName] =
        change.objectType.split('<')[0]?.split('::') || [];

    if (change.display?.data)
        return (
            <ObjectChangeDisplay
                display={change.display.data}
                objectId={change.objectId}
            />
        );
    return (
        <Disclosure>
            {({ open }) => (
                <div className="flex flex-col gap-1">
                    <div className="grid grid-cols-2 overflow-auto cursor-pointer">
                        <Disclosure.Button className="flex items-center cursor-pointer border-none bg-transparent ouline-none p-0 gap-1 text-steel-dark hover:text-steel-darker select-none">
                            <Text variant="pBody" weight="medium">
                                Object
                            </Text>
                            {open ? (
                                <ChevronDown12 className="text-gray-45" />
                            ) : (
                                <ChevronRight12 className="text-gray-45" />
                            )}
                        </Disclosure.Button>
                        <div className="justify-self-end">
                            <ExplorerLink
                                type={ExplorerLinkType.object}
                                objectID={change.objectId}
                                className="text-hero-dark no-underline"
                            >
                                <Text
                                    variant="body"
                                    weight="medium"
                                    truncate
                                    mono
                                >
                                    {formatAddress(change.objectId)}
                                </Text>
                            </ExplorerLink>
                        </div>
                    </div>
                    <Disclosure.Panel>
                        <div className="flex flex-col gap-1">
                            <div className="grid grid-cols-2 overflow-auto relative">
                                <Text
                                    variant="pBody"
                                    weight="medium"
                                    color="steel-dark"
                                >
                                    Package
                                </Text>
                                <div className="flex justify-end">
                                    <ExplorerLink
                                        type={ExplorerLinkType.object}
                                        objectID={packageId}
                                        className="text-hero-dark text-captionSmall no-underline justify-self-end overflow-auto"
                                    >
                                        <Text
                                            variant="pBody"
                                            weight="medium"
                                            truncate
                                            mono
                                        >
                                            {packageId}
                                        </Text>
                                    </ExplorerLink>
                                </div>
                            </div>
                            <div className="grid grid-cols-2 overflow-auto">
                                <Text
                                    variant="pBody"
                                    weight="medium"
                                    color="steel-dark"
                                >
                                    Module
                                </Text>
                                <div className="flex justify-end">
                                    <ExplorerLink
                                        type={ExplorerLinkType.object}
                                        objectID={packageId}
                                        moduleName={moduleName}
                                        className="text-hero-dark no-underline justify-self-end overflow-auto"
                                    >
                                        <Text
                                            variant="pBody"
                                            weight="medium"
                                            truncate
                                            mono
                                        >
                                            {moduleName}
                                        </Text>
                                    </ExplorerLink>
                                </div>
                            </div>
                            <div className="grid grid-cols-2 overflow-auto">
                                <Text
                                    variant="pBody"
                                    weight="medium"
                                    color="steel-dark"
                                >
                                    Type
                                </Text>
                                <div className="flex justify-end">
                                    <ExplorerLink
                                        type={ExplorerLinkType.object}
                                        objectID={packageId}
                                        moduleName={moduleName}
                                        className="text-hero-dark no-underline justify-self-end overflow-auto"
                                    >
                                        <Text
                                            variant="pBody"
                                            weight="medium"
                                            truncate
                                            mono
                                        >
                                            {typeName}
                                        </Text>
                                    </ExplorerLink>
                                </div>
                            </div>
                        </div>
                    </Disclosure.Panel>
                </div>
            )}
        </Disclosure>
    );
}
