import React, { useState } from "react";
import TypeDef from "./typedef";
import Markdown from "markdown-to-jsx";

export const Parameter = (props) => {
  //const desc = String( compile(param.description[0].toUpperCase() + param.description.substring(1, param.description.length)))
  const [type, setType] = useState();
  const [typeDefVisible, setTypeDefVisible] = useState(false);

  const { param, method, schemas } = props;
  const desc = param.description
    ? `${param.description[0].toUpperCase()}${param.description.substring(
        1,
        param.description.length
      )}`
        .replaceAll(/\</g, "&lt;")
        .replaceAll(/\{/g, "&#123;")
    : "";
  const methods = Object.keys(schemas);

  let typename = "";

  if (param.schema && param.schema["$ref"]) {
    typename = param.schema["$ref"].substring(
      param.schema["$ref"].lastIndexOf("/") + 1,
      param.schema["$ref"].length
    );
  } else if (param.schema && !param.schema["$ref"]) {
    if (param.schema.type) {
      if (param.schema.type == "integer") {
        typename = param.schema.format;
      }
      if (param.schema.type == "array") {
        if (param.schema.items) {
          if (param.schema.items["$ref"]) {
            typename = param.schema.items["$ref"].substring(
              param.schema.items["$ref"].lastIndexOf("/") + 1,
              param.schema.items["$ref"].length
            );
          } else if (param.schema.items.format) {
            typename = param.schema.items.format;
          }
        }
      } else {
        typename = param.schema.type;
      }
    }
  }

  let hasSchema = methods.includes(typename);

  const handleClick = (e) => {
    const selType = e.target.innerText.replace("<", "").replace(">", "");
    if (selType == type) {
      setTypeDefVisible(!typeDefVisible);
    } else {
      setTypeDefVisible(true);
    }
    setType(selType);
    //setType(typename);
    //setTypeDefVisible(!typeDefVisible);
    //console.log(schemas[selType])
  };

  return (
    <>
      <div className="grid grid-cols-6 ml-4 odd:bg-sui-ghost-white dark:odd:bg-sui-ghost-dark">
        <div className="rounded-tl-lg rounded-bl-lg col-span-2 p-2 overflow-x-auto">
          {param.name}
          <div className="inline" onClick={hasSchema ? handleClick : undefined}>
            &lt;
            <span
              className={`${
                hasSchema
                  ? "underline decoration-dotted underline-offset-4 decoration-1 cursor-help"
                  : ""
              } `}
            >
              {typename}
            </span>
            &gt;
          </div>
        </div>
        <div className="p-2">{param.required ? "Yes" : "No"}</div>
        <div className="rounded-tr-lg rounded-br-lg col-span-3 p-2 overflow-x-auto">
          {param.description && <Markdown>{desc}</Markdown>}
        </div>
      </div>
      {typeDefVisible && (
        <div className="border text-sm border-solid p-4 pt-0 mx-8 rounded-lg">
          <TypeDef schema={type} schemas={schemas} />
        </div>
      )}
    </>
  );
};

const Parameters = (props) => {
  const { params, method, schemas } = props;
  const hasParams = params.length > 0;

  return (
    <div className="border-b mb-8">
      {hasParams && (
        <div className="grid grid-cols-6 ml-4">
          <div className="rounded-tl-lg rounded-bl-lg col-span-2 p-2 bg-sui-blue dark:bg-sui-blue-dark text-sui-gray-95 dark:text-sui-gray-50 font-bold">
            Name&lt;Type&gt;
          </div>
          <div className="p-2 bg-sui-blue dark:bg-sui-blue-dark text-sui-gray-95 dark:text-sui-gray-50 text-sui-gray-35 font-bold">
            Required
          </div>
          <div className="rounded-tr-lg rounded-br-lg col-span-3 p-2 bg-sui-blue dark:bg-sui-blue-dark text-sui-gray-95 dark:text-sui-gray-50 text-sui-gray-35 font-bold">
            Description
          </div>
        </div>
      )}
      {hasParams &&
        params.map((param) => {
          return (
            <Parameter
              param={param}
              method={method}
              schemas={schemas}
              key={`${method}-${param.name.replaceAll(/\s/g, "-")}`}
            />
          );
        })}

      {!hasParams && <p className="ml-4">None</p>}
    </div>
  );
};

export default Parameters;
