import React from 'react';
import Parameters from './parameters';
import Result from './result';
import Examples from './examples';
import Markdown from 'markdown-to-jsx';


const Method = (props) => {

    const {json, apis, schemas} = props;

    return ( 
        <div>
            {apis.map(api => {
                return (
                    <div key={`div-${api.replaceAll(/\s/g, '-').toLowerCase()}`}>
                    <h2 
                        id={`${api.replaceAll(/\s/g, '-').toLowerCase()}`} 
                        className="pt-12 border-b scroll-mt-32 text-3xl text-sui-blue-dark dark:text-sui-blue font-bold mt-12"
                        key={api.replaceAll(/\s/g, '-').toLowerCase()}>
                            {api}
                    </h2>
                    {json["methods"]
                    .filter(method => method.tags[0].name == api)
                    .map((method)=> {
                        const desc = method.description ? method.description.replaceAll(/\</g, '&lt;').replaceAll(/\{/g, '&#123;') : '' ;
                        return (
                            
                            <div 
                              className={`snap-x ${method.deprecated ? 'bg-sui-warning-light p-8 pt-4 rounded-lg mt-8 dark:bg-sui-warning-dark' : 'pt-8'}`}
                              key={`div-${api.replaceAll(/\s/g, '-').toLowerCase()}-${method.name.toLowerCase()}`}>
                                <h3 
                                    className="snap-start scroll-mt-32 text-2xl font-bold" 
                                    key={`link-${method.name.toLowerCase()}`}
                                    id={`${method.name.toLowerCase()}`}>
                                        {method.name}
                                </h3>
                                {method.deprecated && <div className="p-4 bg-sui-issue rounded-lg font-bold mt-4">Deprecated</div>}
                                <div className="">
                                    <p className="mb-8">
                                        <Markdown>{desc}</Markdown>
                                    </p>
                                    <p className='font-bold mt-4 mb-2 text-xl text-sui-grey-80 dark:text-sui-gray-70'>Parameters</p>
                                    <Parameters method={method.name.toLowerCase()} params={method.params} schemas={schemas} />
                                    <p className='font-bold mb-2 text-xl text-sui-gray-80 dark:text-sui-gray-70'>Result</p>
                                    <Result result={method.result} json={json}/>
                                    {method.examples && <><p className='font-bold text-xl text-sui-gray-80 dark:text-sui-gray-70'>Example</p><Examples examples={method.examples}/></>}
                                </div>
                            </div>
                            
                        )
                    })}
                    </div>
                )
            })}
        </div>
    )
}

export default Method;