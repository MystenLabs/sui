// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ErrorBoundary } from '../../../components/error-boundary/ErrorBoundary';
import { type DataType } from '../ObjectResultType';
import { TableHeader } from '~/ui/TableHeader';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';



function ObjectView({ data }: { data: DataType }) {
    console.log(data)

    return (
        <TabGroup size="lg">
        <TabList>
            <Tab>Details</Tab>
 
        </TabList>
        <TabPanels>
            <TabPanel>
                <div
                    className= 'block grid-cols-1 gap-0 md:grid md:grid-cols-1 md:gap-3'
                    
                >
                    <section
                        className='md:ml-4 block grid-cols-1 gap-0 md:grid md:grid-cols-1 md:gap-3'
                        
                        data-testid="transaction-timestamp"
                    >
                     
                    </section>

                 
                </div>
              
            </TabPanel>
            
            <TabPanel>
                
            </TabPanel>
        </TabPanels>
    </TabGroup>
    );
}

export default ObjectView;
