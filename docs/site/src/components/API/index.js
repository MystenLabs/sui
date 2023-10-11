import React from "react";

import RefNav from "./api-ref/refnav";
import Methods from "./api-ref/method";

import openrpc from "../../open-spec/openrpc.json";

export function getRef (url) {
    return url.substring(url.lastIndexOf('/') + 1, url.length);
    let schemas = Object.keys(openrpc.components.schemas);
    if (schemas.includes(method)){
        return {name: method, schema: openrpc.components.schemas[method]};
    } else {
        return false;
    }
}

const Rpc = () => {
    //console.log(parseOpenRPCDocument(openrpc))

  const apis = [
    ...new Set(openrpc["methods"].map((api) => api.tags[0].name)),
  ].sort();
  const schemas = openrpc.components.schemas;

  if (!openrpc) {
    return <p>Open RPC file not found.</p>;
  }

  let ids = [];
    openrpc["methods"].map((method) => {
        ids.push(method.name.replaceAll(/\s/g, '-').toLowerCase());
    })

  return (
    <div className="mx-4 flex flex-row">

        <div className="pt-12 w-1/4 mb-24 flex-none max-h-screen overflow-y-auto sticky top-12">
        
            <RefNav json={openrpc} apis={apis} />
            
        </div>
        
            <main className="flex-grow w-3/4">
                <div className="mx-8">
                    <div className="">
                        <h1 className="fixed bg-white dark:bg-ifm-background-color-dark w-full py-4 top-14">Sui API Reference - Version: {openrpc.info.version}</h1>
                        
                        <div className="">
                            <Methods json={openrpc} apis={apis} schemas={schemas} />
                        </div>
                        
                    </div>
                </div>
            </main>
        
    </div>
  );
};

export default Rpc;
