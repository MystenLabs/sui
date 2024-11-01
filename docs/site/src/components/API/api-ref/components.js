// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useRef } from "react";
import Link from "@docusaurus/Link";
import Markdown from "markdown-to-jsx";
import { Light as SyntaxHighlighter } from "react-syntax-highlighter";
import js from "react-syntax-highlighter/dist/esm/languages/hljs/json";
import docco from "react-syntax-highlighter/dist/esm/styles/hljs/docco";
import dark from "react-syntax-highlighter/dist/esm/styles/hljs/dracula";
import ScrollSpy from "react-ui-scrollspy";

SyntaxHighlighter.registerLanguage("json", js);

const pillStyle =
  "p-2 border border-solid border-sui-blue-dark rounded-lg max-w-max bg-sui-ghost-white dark:bg-sui-gray-90";

const RefLink = (props) => {
  const { refer } = props;
  const link = refer.substring(refer.lastIndexOf("/") + 1);
  return <Link href={`#${link.toLowerCase()}`}>{link}</Link>;
};

const Of = (props) => {
  const { of, type } = props;
  return (
    <>
      {of.map((o) => {
        if (o["$ref"]) {
          return (
            <div className={`${type === "all" ? "" : "pl-8"}`}>
              <p
                className={`(${o.description} ? "mb-0" : "") ${type === "all" ? "" : pillStyle}`}
              >
                <RefLink refer={o["$ref"]} />
              </p>
              {o.description && (
                <p>
                  <Markdown>{o.description}</Markdown>
                </p>
              )}
            </div>
          );
        } else if (o.type && o.type === "object") {
          return (
            <div className={`${type === "all" ? "" : "pl-8"}`}>
              <p className={`(${o.description} ? "mb-0" : "") ${pillStyle}`}>
                Object
              </p>
              {o.description && (
                <p>
                  <Markdown>{o.description}</Markdown>
                </p>
              )}
              {o.properties && (
                <PropertiesTable
                  properties={Object.entries(o.properties)}
                  schema={o}
                />
              )}
            </div>
          );
        } else if (o.type && o.type === "string") {
          return (
            <div className={`${type === "all" ? "" : "pl-8"}`}>
              <p className={`(${o.description} ? "mb-0" : "") ${pillStyle}`}>
                String{" "}
                {o.enum && o.enum.length > 0 && (
                  <span>
                    enum: [ {o.enum.map((e) => `"${e}"`).join(" | ")} ]
                  </span>
                )}
              </p>
              {o.description && (
                <p>
                  <Markdown>{o.description}</Markdown>
                </p>
              )}
            </div>
          );
        } else if (o.type && o.type === "integer") {
          return (
            <div className={`${type === "all" ? "" : "pl-8"}`}>
              <p className={`(${o.description} ? "mb-0" : "") ${pillStyle}`}>
                {o.type[0].toUpperCase()}
                {o.type.substring(1)}&lt;{o.format}&gt; Minimum: {o.minimum}
              </p>
              {o.description && <Markdown>{o.description}</Markdown>}
            </div>
          );
        } else if (o.type && o.type === "boolean") {
          return (
            <div className={`${type === "all" ? "" : "pl-8"}`}>
              <p className={`(${o.description} ? "mb-0" : "") ${pillStyle}`}>
                Boolean
              </p>
              {o.description && <Markdown>{o.description}</Markdown>}
            </div>
          );
        } else if (o.type && o.type === "array") {
          return (
            <div className={`${type === "all" ? "" : "pl-8"}`}>
              <p className={`(${o.description} ? "mb-0" : "") ${pillStyle}`}>
                [
                {o.items &&
                  Object.keys(o.items).map((k) => {
                    if (k === "$ref") {
                      return <RefLink refer={o.items[k]} />;
                    }
                  })}
                ]
              </p>
              {o.description && (
                <p>
                  <Markdown>{o.description}</Markdown>
                </p>
              )}
            </div>
          );
        } else if (o.anyOf) {
          return <AnyOfInline anyof={o.anyOf} pill />;
        } else if (o.type) {
          return <p>{o.type}</p>;
        }
      })}
    </>
  );
};

const AllOf = (props) => {
  const { allof } = props;
  return (
    <div>
      <Of of={allof} type="all" />
    </div>
  );
};

const AnyOf = (props) => {
  const { anyof } = props;
  return (
    <div>
      <p className="p-2 border border-solid border-sui-blue-dark rounded-lg max-w-max font-bold text-white bg-sui-blue-dark">
        Any of
      </p>
      <div className="ml-1 border-0 border-l-4 border-solid border-sui-blue-dark">
        <Of of={anyof} type="any" />
      </div>
    </div>
  );
};

const AnyOfInline = (props) => {
  const { anyof, pill } = props;
  return (
    <div className={pill && `ml-8 mb-5 ${pillStyle}`}>
      {anyof.map((a, i) => {
        if (a["$ref"]) {
          return (
            <>
              <RefLink refer={a["$ref"]} />
              {i % 2 === 0 ? " | " : ""}
            </>
          );
        }
        if (a.type) {
          return (
            <>
              {a.type}
              {i % 2 === 0 ? " | " : ""}
            </>
          );
        }
      })}
    </div>
  );
};

const OneOf = (props) => {
  const { oneof } = props;
  return (
    <div>
      <p className="p-2 border border-solid border-sui-blue-dark rounded-lg max-w-max font-bold text-white bg-sui-blue-dark">
        One of
      </p>
      <div className="ml-1 border-0 border-l-4 border-solid border-sui-blue-dark">
        <Of of={oneof} type="one" />
      </div>
    </div>
  );
};

const PropertiesTable = (props) => {
  const { properties, schema } = props;
  if (!properties) {
    return;
  }
  return (
    <table className="w-full table table-fixed">
      <thead>
        <tr>
          <th className="">Property</th>
          <th className="">Type</th>
          <th className="w-20">Req?</th>
          <th className="w-1/2">Description</th>
        </tr>
      </thead>
      <tbody>
        {properties.map(([k, v]) => (
          <>
            <tr key={k}>
              <td>{k}</td>
              <td>
                {Array.isArray(v.type) ? v.type.join(" | ") : v.type}
                {v.enum &&
                  ` enum [ ${v.enum.map((e) => `"${e}"`).join(" | ")} ]`}
                {v["$ref"] && <RefLink refer={v["$ref"]} />}
                {v.anyOf && <AnyOfInline anyof={v.anyOf} />}
                {v.allOf && <AllOf allof={v.allOf} />}
                {v.oneOf && "ONEOFCELL"}
                {v === true && "true"}
              </td>
              <td className="text-center">
                {schema.required && schema.required.includes(k) ? "Yes" : "No"}
              </td>
              <td>{v.description && v.description}</td>
            </tr>
            {v.type === "object" ? (
              <tr>
                <td className={`${v.additionalProperties ? "text-right" : ""}`}>
                  {v.additionalProperties && "Additional properties"}
                </td>
                <td colSpan={3}>
                  {v.additionalProperties && v.additionalProperties["$ref"] && (
                    <RefLink refer={v.additionalProperties["$ref"]} />
                  )}
                  {!v.additionalProperties && v.properties && (
                    <PropertiesTable
                      properties={Object.entries(v.properties)}
                      schema={v}
                    ></PropertiesTable>
                  )}
                  {v.additionalProperties &&
                    v.additionalProperties.type &&
                    v.additionalProperties.type}
                  {v.additionalProperties && v.additionalProperties.anyOf && (
                    <AnyOfInline anyof={v.additionalProperties.anyOf} />
                  )}
                  {v.additionalProperties &&
                    v.additionalProperties === true &&
                    "true"}
                </td>
              </tr>
            ) : (
              ""
            )}
          </>
        ))}
      </tbody>
    </table>
  );
};

const Components = (props) => {
  const { schemas } = props;
  const names = Object.keys(schemas);
  const parentScrollContainerRef = () => {
    (useRef < React.HTMLDivElement) | (null > null);
  };
  return (
    <div ref={parentScrollContainerRef()}>
      <h1>Component schemas</h1>
      <ScrollSpy parentScrollContainerRef={parentScrollContainerRef()}>
        {names &&
          names.map((name) => {
            return (
              <div
                key={name}
                className="p-4 m-4 mt-8 snap-start scroll-mt-40 border border-sui-gray-50 border-solid rounded-lg"
                id={name.toLowerCase()}
              >
                <h2>{name}</h2>

                {schemas[name].description && (
                  <p>
                    <Markdown>{schemas[name].description}</Markdown>
                  </p>
                )}
                {schemas[name].type && (
                  <p className="p-2 border border-solid border-sui-blue-dark rounded-lg max-w-max font-bold text-white bg-sui-blue-dark">
                    {schemas[name].type[0].toUpperCase()}
                    {schemas[name].type.substring(1)}
                    {schemas[name].enum &&
                      ` enum [ ${schemas[name].enum.map((e) => `"${e}"`).join(" | ")} ]`}
                  </p>
                )}

                {schemas[name].properties && (
                  <PropertiesTable
                    properties={Object.entries(schemas[name].properties)}
                    schema={schemas[name]}
                  />
                )}
                {schemas[name].allOf && <AllOf allof={schemas[name].allOf} />}
                {schemas[name].oneOf && <OneOf oneof={schemas[name].oneOf} />}
                {schemas[name].anyOf && <AnyOf anyof={schemas[name].anyOf} />}
                {schemas[name]["$ref"] && (
                  <RefLink refer={schemas[name]["$ref"]} />
                )}
                <details className="py-4">
                  <summary>
                    <span className="cursor-pointer">Toggle raw JSON</span>
                  </summary>
                  <pre>
                    <code>{`"${name}":  ${JSON.stringify(schemas[name], null, 2)}`}</code>
                  </pre>
                </details>
              </div>
            );
          })}
      </ScrollSpy>
    </div>
  );
};

export default Components;
