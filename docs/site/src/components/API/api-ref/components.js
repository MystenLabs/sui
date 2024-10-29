// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useState, useEffect } from "react";
import Link from '@docusaurus/Link';
import Markdown from "markdown-to-jsx";
import { Light as SyntaxHighlighter } from "react-syntax-highlighter";
import js from "react-syntax-highlighter/dist/esm/languages/hljs/json";
import docco from "react-syntax-highlighter/dist/esm/styles/hljs/docco";
import dark from "react-syntax-highlighter/dist/esm/styles/hljs/dracula";

SyntaxHighlighter.registerLanguage("json", js);

const RefLink = (props) => {
  const {refer} = props;
  const link = refer.substring(refer.lastIndexOf("/") + 1);
  return <Link href={`#${link.toLowerCase()}`}>{link}</Link>
}

const Of = (props) => {
  const { of } = props;
  return (
    <>
  {of.map((o) => {
    if (o["$ref"]){
      return (
      <div>
        <p className="mb-0"><RefLink refer={o["$ref"]}/></p>
        {o.description && <p><Markdown>{o.description}</Markdown></p>}
      </div>);
    }
    else if (o.type && o.type === "object"){
      return (
        <div>
          <p className="mb-0">Object</p>
          {o.description && <p><Markdown>{o.description}</Markdown></p>}
          {o.properties && <PropertiesTable properties={Object.entries(o.properties)} schema={o}/>}
        </div>
      );
    }
    else if (o.type && o.type === "string"){
      return (
        <div>
          <p className="mb-0">String {o.enum && o.enum.length > 0 && (<span>enum: [{o.enum.map((e) => `"${e}"`).join(" | ")}]</span>)}</p>
          {o.description && <p><Markdown>{o.description}</Markdown></p>}
        </div>
      )
    }
    else if (o.type && o.type === "integer"){
      return (
        <div>
          <p>{o.type[0].toUpperCase()}{o.type.substring(1)}&lt;{o.format}&gt; Minimum: {o.minimum}</p>
          {o.description && <Markdown>{o.description}</Markdown>}
        </div>
      )
    }
    else if (o.type && o.type === "boolean"){
      return (
        <div>
          <p className="pb-0">Boolean</p>
          {o.description && <Markdown>{o.description}</Markdown>}
        </div>
      );
    }
    else if (o.type && o.type === "array"){
      return (
      <div>
        <p className="mb-0">[ {o.items && Object.keys(o.items).map((k) => {
        if (k === "$ref") {
          return <RefLink refer={o.items[k]}/>
        }
  })} ]</p>
      {o.description && <p><Markdown>{o.description}</Markdown></p>}
      </div>);
    }
    else if (o.type) {
      return <p>BANANA - {o.type}</p>
    }
    
  })}</>)
}

const AllOf = (props) => {
  const {allof} = props;
  return (<div>
    <p>All of</p>
    <Of of={allof}/>
  </div>);
}

const AnyOf = (props) => {
  const {anyof} = props;
  return (<div>
    <p>Any of</p>
    <Of of={anyof}/>
  </div>);
}

const OneOf = (props) => {
  const {oneof} = props;
  return (
    <div>
      <p>One of</p>
      <Of of={oneof}/>
    </div>);
}

const PropertiesTable = (props) => {
  const { properties, schema } = props;
  if (!properties){
    return;
  }
  return (
    <table className="w-full table">
      <thead>
        <tr>
          <th>Property</th>
          <th>Type</th>
          <th>Req?</th>
          <th>Description</th>
        </tr>
      </thead>
      <tbody>
        {properties.map(([k, v]) => (
          
          <tr key={k}>{console.log({v})}
            <td>{k}</td>
            <td>{v.type && v.type}{v["$ref"] && <RefLink refer={v["$ref"]}/>}{v.anyOf && "ANY"}{v.allOf && "ALL"}{v.oneOf && "ONE"}</td>
            <td>{schema.required && (schema.required.includes(k)) ? "Yes" : "No"}</td>
            <td>{v.description}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

const Components = (props) => {
  const { schemas } = props;
  const names = Object.keys(schemas);
  return (
    <>
    <h1>Components</h1>
    {names && names.map((name) => {
      return (
        <div key={name} className="py-4 my-4 border border-sui-blue border-solid rounded-lg">
          <h2 id={name.toLowerCase()}>{name}</h2>
          {schemas[name].description && <p>
            <Markdown>{schemas[name].description}</Markdown>
          </p> }
          {schemas[name].type && <p className="bg-sui-blue-dark text-white font-bold">
            {schemas[name].type}
          </p> }
          
          {schemas[name].properties && <PropertiesTable properties={Object.entries(schemas[name].properties)} schema={schemas[name]}/>}
          {schemas[name].allOf && <AllOf allof={schemas[name].allOf}/>}
          {schemas[name].oneOf && <OneOf oneof={schemas[name].oneOf}/>}
          {schemas[name].anyOf && <AnyOf anyof={schemas[name].anyOf}/>}
          {schemas[name]["$ref"] && <RefLink refer={schemas[name]["$ref"]}/>}
        </div>
      )
    })}
    </>
  )
}

export default Components;