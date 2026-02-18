// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Markdown from "markdown-to-jsx";
import PropType from "./proptype";

export const Parameter = (props) => {
  const { param, schemas } = props;

  const desc = param.description
    ? `${param.description[0].toUpperCase()}${param.description.substring(1)}`
        .replaceAll(/\</g, "&lt;")
        .replaceAll(/\{/g, "&#123;")
    : "";

  return (
    <div className="api-row">
      <div className="api-cell api-cell-scroll">
        <PropType proptype={[param.name, param.schema]} />
      </div>

      <div className="api-cell">
        <span className={param.required ? "api-badge-yes" : "api-badge-no"}>
          {param.required ? "Required" : "Optional"}
        </span>
      </div>

      <div className="api-cell api-cell-scroll">
        {param.description ? <Markdown>{desc}</Markdown> : <span className="api-muted">—</span>}
      </div>
    </div>
  );
};

const Parameters = (props) => {
  const { params, method, schemas } = props;
  const hasParams = params.length > 0;

  return (
    <div className="api-card api-card-pad">
      <div className="api-section-title">Parameters</div>

      {!hasParams && <div className="api-muted">None</div>}

      {hasParams && (
        <>
          <div className="api-row-head">
            <div>Name &amp; Type</div>
            <div>Required</div>
            <div>Description</div>
          </div>

          <div className="api-rows">
            {params.map((param) => (
              <Parameter
                param={param}
                method={method}
                schemas={schemas}
                key={`${method}-${param.name.replaceAll(/\s/g, "-")}`}
              />
            ))}
          </div>
        </>
      )}
    </div>
  );
};

export default Parameters;
