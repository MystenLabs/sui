// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useRpc } from '~/hooks/useRpc';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

function FunctionView({
    pkgId,
    selectedModuleName,
}: {
    pkgId: string;
    selectedModuleName: string;
}) {
    const rpc = useRpc();

    const { data } = useQuery(
        ['get-function-data', pkgId, selectedModuleName],
        async () => {
            return await rpc.getNormalizedMoveModule(pkgId, selectedModuleName);
        }
    );

    if (!!data) {
        return (
            <div className="border-0 md:border-l border-solid border-sui-grey-45 md:pl-7 pt-5 grow overflow-auto">
                <TabGroup size="md">
                    <TabList>
                        <Tab>Simulate &amp; Execute</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div className="overflow-auto h-verticalListLong">
                                <div>
                                    <div>
                                        <div>
                                            {Object.entries(
                                                data.exposed_functions
                                            ).map(([fnName, fnData]) => (
                                                <div
                                                    key={fnName}
                                                    className="bg-sui-grey-40 mb-2.5 px-5 py-4 rounded-lg text-body text-sui-grey-90"
                                                >
                                                    <div className="font-semibold">
                                                        {fnName}
                                                    </div>
                                                    <div>
                                                        {fnData.parameters.map(
                                                            (
                                                                argData,
                                                                index
                                                            ) => (
                                                                <div
                                                                    key={index}
                                                                    className="pl-2.5 mt-4"
                                                                >
                                                                    {fnData
                                                                        .type_parameters[
                                                                        argData
                                                                            .TypeParameter
                                                                    ]
                                                                        ? fnData.type_parameters[
                                                                              argData
                                                                                  .TypeParameter
                                                                          ].abilities.join(
                                                                              ', '
                                                                          )
                                                                        : JSON.stringify(
                                                                              argData
                                                                          )}
                                                                </div>
                                                            )
                                                        )}
                                                    </div>
                                                </div>
                                            ))}
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
            </div>
        );
    }

    return <div>{JSON.stringify(data)}</div>;
}

export default function FunctionViewWrapper({
    pkgId,
    selectedModuleName,
}: {
    pkgId: string | undefined;
    selectedModuleName: string;
}) {
    if (!!pkgId) {
        return (
            <FunctionView
                pkgId={pkgId}
                selectedModuleName={selectedModuleName}
            />
        );
    } else {
        return <div />;
    }
}
