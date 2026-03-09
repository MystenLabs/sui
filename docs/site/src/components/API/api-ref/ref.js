// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import React from "react";
import Markdown from "markdown-to-jsx";
import PropType from "./proptype";

const Ref = (props) => {
  const { schema } = props;
  const requireds = typeof schema.required !== "undefined" ? schema.required : [];

  return (
    <div className="api-card api-card-pad">
      {schema.description && (
        <div className="api-muted">
          <Markdown>{schema.description}</Markdown>
        </div>
      )}

      {schema.properties && (
        <div className="api-section">
          <div className="api-section-title">Properties</div>

          <div className="api-row-head">
            <div>Parameter</div>
            <div>Required</div>
            <div>Description</div>
          </div>

          <div className="api-rows">
            {Object.entries(schema.properties).map(([name, def], idx) => (
              <div key={idx} className="api-row">
                <div className="api-cell api-cell-scroll">
                  <PropType proptype={[name, def]} />
                </div>
                <div className="api-cell">
                  <span className={requireds.includes(name) ? "api-badge-yes" : "api-badge-no"}>
                    {requireds.includes(name) ? "Required" : "Optional"}
                  </span>
                </div>
                <div className="api-cell api-cell-scroll">
                  {def.description ? <Markdown>{def.description}</Markdown> : <span className="api-muted">—</span>}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

export default Ref;
