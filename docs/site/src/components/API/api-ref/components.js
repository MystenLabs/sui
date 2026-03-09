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

const RefLink = ({ refer }) => {
  const link = refer.substring(refer.lastIndexOf("/") + 1);
  return <Link href={`#${link.toLowerCase()}`}>{link}</Link>;
};

const Of = ({ of, type }) => {
  return (
    <div className="grid gap-2">
      {of.map((o, idx) => {
        const indent = type === "all" ? "" : "pl-4";

        if (o["$ref"]) {
          return (
            <div key={idx} className={indent}>
              <div className="api-chip">
                <RefLink refer={o["$ref"]} />
              </div>
              {o.description && (
                <div className="mt-2 api-muted">
                  <Markdown>{o.description}</Markdown>
                </div>
              )}
            </div>
          );
        }

        if (o.type === "object") {
          return (
            <div key={idx} className={indent}>
              <div className="api-chip">Object</div>
              {o.description && (
                <div className="mt-2 api-muted">
                  <Markdown>{o.description}</Markdown>
                </div>
              )}
              {o.properties && (
                <PropertiesRows properties={Object.entries(o.properties)} schema={o} />
              )}
            </div>
          );
        }

        if (o.type === "string") {
          return (
            <div key={idx} className={indent}>
              <div className="api-chip">
                String {o.enum?.length ? <span className="api-muted">enum</span> : null}
              </div>
              {o.enum?.length ? (
                <div className="mt-2 api-typechip">
                  [ {o.enum.map((e) => `"${e}"`).join(" | ")} ]
                </div>
              ) : null}
              {o.description && (
                <div className="mt-2 api-muted">
                  <Markdown>{o.description}</Markdown>
                </div>
              )}
            </div>
          );
        }

        if (o.type === "integer") {
          return (
            <div key={idx} className={indent}>
              <div className="api-chip">
                Integer&lt;{o.format}&gt; {typeof o.minimum !== "undefined" ? `min: ${o.minimum}` : ""}
              </div>
              {o.description && <div className="mt-2 api-muted"><Markdown>{o.description}</Markdown></div>}
            </div>
          );
        }

        if (o.type === "boolean") {
          return (
            <div key={idx} className={indent}>
              <div className="api-chip">Boolean</div>
              {o.description && <div className="mt-2 api-muted"><Markdown>{o.description}</Markdown></div>}
            </div>
          );
        }

        if (o.type === "array") {
          return (
            <div key={idx} className={indent}>
              <div className="api-chip">
                [
                {o.items &&
                  Object.keys(o.items).map((k) => (k === "$ref" ? <RefLink key={k} refer={o.items[k]} /> : null))}
                ]
              </div>
              {o.description && <div className="mt-2 api-muted"><Markdown>{o.description}</Markdown></div>}
            </div>
          );
        }

        if (o.anyOf) return <AnyOfInline key={idx} anyof={o.anyOf} pill />;

        if (o.type) return <div key={idx} className="api-muted">{o.type}</div>;

        return null;
      })}
    </div>
  );
};

const AnyOfInline = ({ anyof, pill }) => {
  return (
    <div className={pill ? "api-typechip ml-4 mb-2" : ""}>
      {anyof.map((a, i) => (
        <React.Fragment key={i}>
          {a["$ref"] ? <RefLink refer={a["$ref"]} /> : a.type ?? ""}
          {i < anyof.length - 1 ? " | " : ""}
        </React.Fragment>
      ))}
    </div>
  );
};

const AllOf = ({ allof }) => <Of of={allof} type="all" />;

const AnyOf = ({ anyof }) => (
  <div className="api-card api-card-pad">
    <div className="flex items-center gap-2">
      <span className="api-chip">Any of</span>
      <span className="api-muted">Union type</span>
    </div>
    <div className="mt-3 border-l-4 pl-3" style={{ borderColor: "rgba(41,141,255,0.5)" }}>
      <Of of={anyof} type="any" />
    </div>
  </div>
);

const OneOf = ({ oneof }) => (
  <div className="api-card api-card-pad">
    <div className="flex items-center gap-2">
      <span className="api-chip">One of</span>
      <span className="api-muted">Exclusive union</span>
    </div>
    <div className="mt-3 border-l-4 pl-3" style={{ borderColor: "rgba(41,141,255,0.5)" }}>
      <Of of={oneof} type="one" />
    </div>
  </div>
);

const PropertiesRows = ({ properties, schema }) => {
  if (!properties) return null;

  return (
    <div className="mt-3">
      <div className="api-row-head">
        <div>Property</div>
        <div>Required</div>
        <div>Description</div>
      </div>

      <div className="api-rows">
        {properties.map(([k, v]) => (
          <div key={k} className="api-row">
            <div className="api-cell api-cell-scroll">
              <div className="flex flex-col gap-1">
                <div className="font-semibold">{k}</div>
                <div className="api-typechip">
                  {Array.isArray(v.type) ? v.type.join(" | ") : v.type}
                  {v.enum ? ` enum [ ${v.enum.map((e) => `"${e}"`).join(" | ")} ]` : ""}
                  {v["$ref"] ? " " : ""}
                  {v["$ref"] ? <RefLink refer={v["$ref"]} /> : null}
                  {v.anyOf ? <span> {buildInline(v.anyOf)} </span> : null}
                </div>
              </div>
            </div>

            <div className="api-cell">
              <span className={schema.required?.includes(k) ? "api-badge-yes" : "api-badge-no"}>
                {schema.required?.includes(k) ? "Required" : "Optional"}
              </span>
            </div>

            <div className="api-cell api-cell-scroll">
              {v.description ? v.description : <span className="api-muted">—</span>}
            </div>

            {v.type === "object" ? (
              <div className="api-cell api-cell-scroll" style={{ gridColumn: "1 / -1" }}>
                <div className="mt-2 api-card api-card-pad">
                  {v.additionalProperties ? (
                    <div className="api-muted">
                      <span className="font-semibold">Additional properties:</span>{" "}
                      {v.additionalProperties["$ref"] ? (
                        <RefLink refer={v.additionalProperties["$ref"]} />
                      ) : v.additionalProperties.type ? (
                        v.additionalProperties.type
                      ) : v.additionalProperties.anyOf ? (
                        buildInline(v.additionalProperties.anyOf)
                      ) : (
                        "true"
                      )}
                    </div>
                  ) : v.properties ? (
                    <PropertiesRows properties={Object.entries(v.properties)} schema={v} />
                  ) : null}
                </div>
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
};

const buildInline = (anyof) => anyof.map((a) => (a["$ref"] ? a["$ref"].split("/").pop() : a.type)).join(" | ");

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
        {names.map((name) => (
          <div
            key={name}
            className="api-card api-card-pad-lg mt-6 snap-start scroll-mt-40"
            id={name.toLowerCase()}
          >
            <div className="flex items-start justify-between gap-3">
              <h2 className="m-0">{name}</h2>
              <span className="api-chip">Schema</span>
            </div>

            {schemas[name].description && (
              <div className="mt-3 api-muted">
                <Markdown>{schemas[name].description}</Markdown>
              </div>
            )}

            {schemas[name].type && (
              <div className="mt-3">
                <span className="api-chip">
                  {schemas[name].type[0].toUpperCase()}
                  {schemas[name].type.substring(1)}
                </span>
                {schemas[name].enum?.length ? (
                  <span className="ml-2 api-typechip">
                    enum [ {schemas[name].enum.map((e) => `"${e}"`).join(" | ")} ]
                  </span>
                ) : null}
              </div>
            )}

            {schemas[name].properties && (
              <PropertiesRows properties={Object.entries(schemas[name].properties)} schema={schemas[name]} />
            )}

            {schemas[name].allOf && <div className="mt-4"><AllOf allof={schemas[name].allOf} /></div>}
            {schemas[name].oneOf && <div className="mt-4"><OneOf oneof={schemas[name].oneOf} /></div>}
            {schemas[name].anyOf && <div className="mt-4"><AnyOf anyof={schemas[name].anyOf} /></div>}

            {schemas[name]["$ref"] && (
              <div className="mt-3">
                <RefLink refer={schemas[name]["$ref"]} />
              </div>
            )}

            <details className="mt-4">
              <summary className="cursor-pointer font-semibold">Raw JSON</summary>
              <div className="api-code mt-3">
                <div className="api-code-title">{name}</div>
                <div className="api-code-body">
                  <pre style={{ margin: 0, padding: 12 }}>
                    <code>{`"${name}": ${JSON.stringify(schemas[name], null, 2)}`}</code>
                  </pre>
                </div>
              </div>
            </details>
          </div>
        ))}
      </ScrollSpy>
    </div>
  );
};

export default Components;
