import React, { useState, createRef } from 'react';
import Link from '@docusaurus/Link';
import NetworkSelect from './networkselect';
import ScrollSpy from "react-scrollspy-navigation";

const RefNav = (props) => {

    const {json, apis} = props;
       
    return (
        
        <div className="mb-24">
            <div className="sticky -top-9 -mt-9 pt-9 pb-2 bg-white dark:bg-ifm-background-color-dark">
                <NetworkSelect/>
            </div>
            <ScrollSpy>
        { apis.map(api => { return (
            
               <>  
            <Link 
                href={`#${api.replaceAll(/\s/g, '-').toLowerCase()}`}
                key={`${api.replaceAll(/\s/g, '-').toLowerCase()}`}
                className="hover:no-underline pt-4 block text-black dark:text-white hover:text-sui-blue dark:hover:text-sui-blue"
                ref={createRef()}
            >
                {api}
            </Link>
            {json["methods"]
                .filter(method => method.tags[0].name == api)
                .map((method)=> {
                    return (
                        <Link 
                            className="my-1 ml-2 block text-sui-gray-95 dark:text-sui-grey-35 hover:no-underline dark:hover:text-sui-blue" 
                            key={`link-${method.name.toLowerCase()}`}
                            href={`#${method.name.toLowerCase()}`}
                            ref={createRef()}
                        >
                                {method.name}
                        </Link>
                    )
            })}
            </>
        )})}
        </ScrollSpy>
    </div>
    )
    
}

export default RefNav;