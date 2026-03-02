// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React, { useRef } from "react";
import { useHistory } from "@docusaurus/router";
import Parameters from "./parameters";
import Result from "./result";
import Examples from "./examples";
import Markdown from "markdown-to-jsx";
import ScrollSpy from "react-ui-scrollspy";

const Method = (props) => {
  const { json, apis, schemas } = props;
  const history = useHistory();

  const parentScrollContainerRef = () => {
    (useRef < React.HTMLDivElement) | (null > null);
  };

  const handleClick = (e) => {
    let href = "#";
    if (!e.target.nodeName.match(/^H/)) return;
    href += e.target.id ? e.target.id : e.target.parentNode.id;
    history.push(href);
  };

  return (
    <>
      {apis.map((api) => {
        const apiId = api.replaceAll(/\s/g, "-").toLowerCase();

        return (
          <div key={`div-${apiId}`} ref={parentScrollContainerRef()}>
            <h2
              id={apiId}
              className="scroll-mt-32 text-3xl font-extrabold mt-12 cursor-pointer"
              onClick={handleClick}
            >
              {api}
            </h2>

            <ScrollSpy parentScrollContainerRef={parentScrollContainerRef()}>
              {json["methods"]
                .filter((method) => method.tags[0].name === api)
                .map((method) => {
                  const desc = method.description
                    ? method.description.replaceAll(/\</g, "&lt;").replaceAll(/\{/g, "&#123;")
                    : "";

                  return (
                    <div
                      key={`div-${apiId}-${method.name.toLowerCase()}`}
                      id={method.name.toLowerCase()}
                      className="snap-start scroll-mt-32 mt-6"
                      onClick={handleClick}
                    >
                      <div className="api-card api-card-pad-lg">
                        <div className="flex items-start justify-between gap-3">
                          <h3 className="text-2xl font-extrabold m-0">{method.name}</h3>
                          {method.deprecated ? <span className="api-badge-warn">Deprecated</span> : null}
                        </div>

                        {desc && (
                          <div className="mt-3 api-muted">
                            <Markdown>{desc}</Markdown>
                          </div>
                        )}

                        <div className="mt-5 grid gap-4">
                          <Parameters
                            method={method.name.toLowerCase()}
                            params={method.params}
                            schemas={schemas}
                          />

                          <Result result={method.result} json={json} />

                          {method.examples && (
                            <Examples method={method.name} examples={method.examples} />
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })}
            </ScrollSpy>
          </div>
        );
      })}
    </>
  );
};

export default Method;
