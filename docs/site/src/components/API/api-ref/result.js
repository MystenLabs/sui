import React from 'react';
import Method from './method';
import Markdown from 'markdown-to-jsx';

const Property = (props) => {
    const { property } = props;
    const desc = property.desc ? (`${property.desc[0].toUpperCase()}${property.desc.substring(1, property.desc.length)}`).replaceAll(/\</g, '&lt;').replaceAll(/\{/g, '&#123;') : '';
    return (
        <div className={`grid grid-cols-6 ml-4 even:bg-sui-ghost-white dark:even:bg-sui-ghost-dark`}>
            <div className="rounded-tl-lg rounded-bl-lg col-span-2 p-2 overflow-x-auto">
                <p className="overflow-x-auto mb-0">{property.name}{property.type}</p>
            </div>
            <div className="p-2">
                <p className="mb-0">{property.req ? "Yes" : "No" }</p>
            </div>
            <div className="rounded-tr-lg rounded-br-lg col-span-3 p-2 overflow-x-auto">
                <p className="mb-0">{property.desc && <Markdown>{desc}</Markdown>}</p>
            </div>
        
        </div>
    )
}

const Result = (props) => {

    const {json, result} = props;
    const hasRef = typeof(result.schema['$ref']) !== "undefined";
    
    const isArray = !hasRef && typeof(result.schema.items) !== 'undefined' && typeof(result.schema.items['$ref']!=='undefined');
    const isArrayWithRef = isArray && typeof(result.schema.type) !== "undefined" && result.schema.type == "array";
    const isObj = !hasRef && typeof(result.schema.type) !== "undefined" && result.schema.type == "object";
    const objRef = typeof(result.schema.additionalProperties) !== 'undefined' && typeof(result.schema.additionalProperties['$ref']) !== "undefined" ? result.schema.additionalProperties['$ref'].substring(result.schema.additionalProperties['$ref'].lastIndexOf('/') + 1, result.schema.additionalProperties['$ref'].length) : "DOC_ERR" ;
    const isInt = !hasRef && typeof(result.schema.type) !== "undefined" && result.schema.type == "integer";

    
    let refObj = {};

    if (hasRef){
       const schemaPath = result.schema['$ref'].substring(result.schema['$ref'].lastIndexOf('/') + 1, result.schema.length);
       const ref = json.components.schemas[schemaPath];
       if (ref.description) {
        refObj.desc = ref.description;
       }
       if (ref.required){
        refObj.reqs = ref.required;
       }
       if (ref.properties){
        let x = 0;
        refObj.properties=[];
        try{
        for (const [k, v] of Object.entries(ref.properties)){
            refObj.properties.push({name: k, type: null, desc: null, req:(refObj.reqs.includes(k))});
            if (typeof(v.type) !== "undefined" && v.type == "array"){
                
                if (typeof(v.items['$ref']) !== "undefined"){
                    refObj.properties[x].type = "<[" + v.items['$ref'].substring(v.items['$ref'].lastIndexOf('/') + 1, v.items['$ref'].length) + "]>";
                } else if (typeof(v.items.type) !== "undefined" && v.items.type === "integer") {
                    refObj.properties[x].type = "<[" + v.items.format + "]>";
                } else if (typeof(v.items.type) !== "undefined" && v.items.type === "array") {
                    let items = [];
                    try{
                        if (typeof(v.items.items['$ref']) !== "undefined") {
                            items.push(`{${v.items.items['$ref'].substring(v.items.items['$ref'].lastIndexOf('/') + 1, v.items.items['$ref'].length)}}`)
                        } else {
                            v.items.items.map( item => {
                            if (typeof(item['$ref']) !== 'undefined'){
                                items.push(`{${item['$ref'].substring(item['$ref'].lastIndexOf('/') + 1, item['$ref'].length)}}`)
                            } else if (typeof(item.type) !== 'undefined') {
                                if (item.type === 'integer'){
                                    items.push(item.format)
                                }
                                
                            }
                            })
                        }
                    } catch(err) {
                        console.log(err)
                        console.log(v)
                    }
                    
                    refObj.properties[x].type = `<[${items.join(', ')}]>`
                } else {
                    console.log("Result not processed")
                    console.log(v)
                }
                
            } else if (typeof(v.type) !== "undefined" && v.type == "integer") {
                refObj.properties[x].type = "<" + v.format + ">";
            } else if (typeof(v.allOf) !== "undefined" && v.allOf.length == 1) {
                if (typeof(v.allOf[0]['$ref']) !== "undefined"){
                    refObj.properties[x].type = "<[" + v.allOf[0]['$ref'].substring(v.allOf[0]['$ref'].lastIndexOf('/') + 1, v.allOf[0]['$ref'].length) + "]>"
                } else {
                    console.log("Error")
                }
            } else if (typeof(v.type) !== "undefined" && v.type == "string") {
                refObj.properties[x].type = "<string>";
            } else if (typeof(v["$ref"]) !== "undefined") {
                refObj.properties[x].type = "<" + v["$ref"].substring(v['$ref'].lastIndexOf('/') + 1, v['$ref'].length) + ">";
            } else if (typeof(v.type) !== "undefined" && v.type == "boolean") {
                refObj.properties[x].type = "<Boolean>";
            } else if (typeof(v.anyOf) !== "undefined") {
                if (typeof(v.anyOf[0]["$ref"]) !== "undefined"){
                    refObj.properties[x].type = "<" + v.anyOf[0]["$ref"].substring(v.anyOf[0]["$ref"].lastIndexOf('/') + 1, v.anyOf[0]["$ref"].length) + " | null>";
                } else {
                    console.log("Error")
                }
            } else if (typeof(v.type) !== "undefined" && v.type == "object") {
                if (typeof(v.additionalProperties["$ref"]) !== "undefined") {
                    refObj.properties[x].type = "<" + v.additionalProperties["$ref"].substring(v.additionalProperties["$ref"].lastIndexOf('/') + 1, v.additionalProperties["$ref"].length) + ">";
                } else {
                    console.log("Error")
                }
                
            } else if (typeof(v.items) !== "undefined" && v.items.type == "array") {
                if (typeof(v.items.items[0]["$ref"]) !== "undefined"){
                    refObj.properties[x].type = "<[" + v.items.items[0]["$ref"].substring(v.items.items[0]["$ref"].lastIndexOf('/') + 1, v.items.items[0]["$ref"].length) + ", " + v.items.items[1].format + "]>";
                } else {
                    console.log("Error")
                }
            } else if (typeof(v.type) !== "undefined" && Array.isArray(v.type)) {
                if (v.type[0] == "string"){
                    refObj.properties[x].type = "<string, null>";
                } else if (v.type[0] == "integer") {
                    refObj.properties[x].type = "<" + v.format + ", null>"
                }
            } else {
                console.log("A Result was not processed:\n")
                console.log(v)
            }
            if (typeof(v.description) !== "undefined"){
                refObj.properties[x].desc = v.description;
            }
            x++;
        }}
        catch (err){
            console.log(err);
        }
        }
    }

    const hasRefProps = refObj.properties && refObj.properties.length > 0;
    const hasDesc = refObj.desc;

    return (
        <>
            {hasRef && 
                <p className="ml-4 p-2 font-bold text-sui-gray-80 dark:text-sui-gray-50 border dark:border-sui-gray-75 rounded-lg bg-sui-ghost-white dark:bg-sui-ghost-dark">{`${result.name}<${result.schema['$ref'].substring(result.schema['$ref'].lastIndexOf('/') + 1, result.schema['$ref'].length)}>`}</p>
            }
            {isArray && 
                <p className="ml-4 p-2 font-bold text-sui-gray-80 dark:text-sui-gray-50 border dark:border-sui-gray-75 rounded-lg bg-sui-ghost-white dark:bg-sui-ghost-dark">{`${result.name}`}{isArrayWithRef && `<[${result.schema.items['$ref'].substring(result.schema.items['$ref'].lastIndexOf('/') + 1, result.schema.items['$ref'].length)}]>`}</p>}
            {isObj &&
                <p className="ml-4 p-2 font-bold text-sui-gray-80 dark:text-sui-gray-50 border dark:border-sui-gray-75 rounded-lg bg-sui-ghost-white dark:bg-sui-ghost-dark">{`${result.name}<{${objRef}}>`}</p>
            }
            {isInt && 
                <p className="ml-4 p-2 font-bold text-sui-gray-80 dark:text-sui-gray-50 border dark:border-sui-gray-75 rounded-lg bg-sui-ghost-white dark:bg-sui-ghost-dark">{`${result.name}<${result.schema.format}>`}</p>
            }
            {hasDesc &&
                <p className="ml-4 p-2 text-sui-gray-100 dark:text-sui-gray-50 rounded-lg"><Markdown>{refObj.desc}</Markdown></p>
            }
            {hasRef && hasRefProps && 
                <div className="border-b pb-4">
                    <p className="font-bold mt-6 mb-2 ml-4 text-sui-gray-80 dark:text-sui-gray-50">Properties</p>
                    <div className={`grid grid-cols-6 ml-4 pb-2`}><div className="rounded-tl-lg rounded-bl-lg col-span-2 p-2 bg-sui-blue dark:bg-sui-blue-dark text-sui-gray-95 dark:text-sui-gray-50 font-bold">Name&lt;Type&gt;</div><div className="p-2 bg-sui-blue dark:bg-sui-blue-dark text-sui-gray-95 dark:text-sui-gray-50 font-bold">Required</div><div className="rounded-tr-lg rounded-br-lg col-span-3 p-2 bg-sui-blue dark:bg-sui-blue-dark text-sui-gray-95 dark:text-sui-gray-50 font-bold">Description</div></div>
                {refObj.properties.map(property => {
                    return <Property property={property} key={property.name} /> 
                })}
                </div>
                
            }
            {!result && <p>Not applicable</p>}
        </>
    )

}

export default Result;


