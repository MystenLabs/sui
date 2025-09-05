// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const protocolInject = async function (source) {
  this.cacheable && this.cacheable();

  const callback = this.async();
  const options = this.getOptions();
  const spec = JSON.parse(options.protocolSpec);
  const toc = [];
  const createId = (name) => {
    return name.replace(/[\._]/g, "-").replace(/\//g, "_");
  };
  const suiSorted = (array) => {
    return array.sort((a, b) => {
      const aStartsWithSui = a.name.startsWith("sui");
      const bStartsWithSui = b.name.startsWith("sui");

      if (aStartsWithSui && !bStartsWithSui) return -1;
      if (!aStartsWithSui && bStartsWithSui) return 1;
      return 0;
    });
  };

  for (const proto of spec.files) {
    let messages = [];
    let protoLink = createId(proto.name);
    if (proto.messages) {
      for (const message of proto.messages) {
        messages.push({
          name: message.name,
          link: createId(message.fullName),
        });
      }
    }
    let services = [];
    if (proto.services) {
      for (const service of proto.services) {
        services.push({
          name: service.name,
          link: createId(service.fullName),
        });
      }
    }
    let enums = [];
    if (proto.enums) {
      for (const num of proto.enums) {
        enums.push({
          name: num.name,
          link: createId(num.fullName),
        });
      }
    }
    let item = { name: proto.name, link: protoLink, messages, services, enums };
    toc.push(item);
  }
  const types = [];
  for (const prototype of spec.scalarValueTypes) {
    types.push({ name: prototype.protoType, link: prototype.protoType });
  }
  let tocSorted = suiSorted(toc);
  tocSorted.push({
    name: "Scalar Value Types",
    link: "scalar-value-types",
    messages: types,
  });
  // Unescaped curly braces mess up docusaurus render
  const handleCurlies = (text) => {
    let isCodeblock = false;

    let final = text.split("\n");
    for (const [idx, line] of final.entries()) {
      if (line.includes("```")) {
        isCodeblock = !isCodeblock;
      }
      if (!isCodeblock) {
        let curlyIndices = [];
        let insideBackticks = false;
        for (let i = 0; i < line.length; i++) {
          if (line[i] === "`") {
            insideBackticks = !insideBackticks;
          }
          if (line[i] === "{" && !insideBackticks) {
            curlyIndices.unshift(i);
          }
        }
        for (const j of curlyIndices) {
          final[idx] = [
            line.substring(0, j),
            "&#123;",
            line.substring(j + 1),
          ].join("");
        }
      }
    }
    return final.join("\n");
  };

  let content = [`<Protocol toc={${JSON.stringify(tocSorted)}}/>`];

  let messageSort = (array) => {
    return array.sort((a, b) => a.name.localeCompare(b.name));
  };
  const files = suiSorted(spec.files);
  const bordercolor = "border-sui-blue-dark dark:border-sui-blue";

  const tabStyle = `grid grid-cols-6 border border-solid border-b-0 border-sui-blue-dark ${bordercolor}`;
  const tabRowStyle = `p-4 border-0 border-b border-solid ${bordercolor} col-span-full`;
  const tabHeaderStyle = `${tabRowStyle} bg-sui-gray-50 dark:bg-sui-ghost-dark`;
  const tabAltHeaderStyle = `${tabRowStyle} bg-sui-ghost-white dark:bg-sui-gray-95`;
  const colHeaderStyle = `p-2 border-0 border-r border-b border-solid col-span-2 flex items-center ${bordercolor} overflow-x-auto`;
  const colCellStyle = `p-2 col-span-4 border-0 border-b border-solid ${bordercolor}`;
  for (const file of files) {
    content.push(`\n## ${file.name} {#${createId(file.name)}}`);
    content.push(
      `<div className="text-lg">\n${handleCurlies(file.description).replace(
        /\n##? (.*)\n/,
        "### $1",
      )}\n</div>`,
    );
    content.push("<div className='ml-4'>");
    if (file.messages.length > 0){
      content.push(`<h4 className="mt-8">Messages</h4>`);
    }
    for (const message of file.messages) {
      let fields = [];
      let oneofFields = [];
      let declarations = [];
      if (message.hasOneofs) {
        for (const field of message.fields) {
          if (field.isoneof === true && !field.oneofdecl.match(/^_/)) {
            oneofFields.push(field);
            if (!declarations.includes(field.oneofdecl)) {
              declarations.push(field.oneofdecl);
            }
          } else {
            fields.push(field);
          }
        }
      } else {
        fields = Object.values(message.fields);
      }
      fields = messageSort(fields);
      oneofFields = messageSort(oneofFields);
      content.push(`<div className="mt-4"></div>`);
      content.push(`\n### ${message.name} {#${createId(message.fullName)}}`);
      content.push(
        `<div className="text-lg">\n${handleCurlies(message.description)
          .replace(/</g, "&#lt;")
          .replace(/^(#{1,2})\s(?!#)/gm, "### ")}\n
        </div>`,
      );

      if (fields.length > 0 || oneofFields.length > 0) {
        content.push(`<div className='${tabStyle}'>`);
        content.push(`<div className='${tabHeaderStyle}'>Fields</div>`);
      }

      if (fields.length > 0) {
        for (const field of fields.entries()) {
          //console.log(field)
          const hasType = field[1].type && field[1].type !== "";
          const hasLabel = field[1].label && field[1].label !== "";
          const hasDesc = field[1].description && field[1].description !== "";
          content.push(
            `<div className="${colHeaderStyle}">${field[1].name}</div>`,
          );
          content.push(`<div className="${colCellStyle}">`);
          if (hasType) {
            content.push(
              `<div className="">[${field[1].type}](#${createId(field[1].fullType)})</div>`,
            );
          }
          if (hasLabel) {
            let label =
              field[1].label[0].toUpperCase() + field[1].label.substring(1);
            let labelBg = "bg-sui-ghost-white dark:bg-sui-ghost-dark";
            if (field[1].label === "optional") {
              label = "Proto3 optional";
              labelBg = "bg-sui-blue-light dark:bg-sui-blue-dark";
            } else if (field[1].label === "repeated") {
              label = "Repeated []";
              labelBg = "bg-sui-warning-light dark:bg-sui-warning-dark";
            }
            content.push(
              `<div className="px-2 py-1 my-1 w-fit border border-solid rounded-full text-sm ${labelBg}">${label}</div>`,
            );
          }
          if (hasDesc) {
            content.push(
              `<div className="">${handleCurlies(field[1].description)
                .replace(/\n\/?/g, " ")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
          }
          content.push("</div>");
        }
      }

      if (declarations.length > 0) {
        for (const declaration of declarations) {
          content.push(
            `<div className='${tabAltHeaderStyle}'>Union field <b>${declaration}</b> can be only one of the following.</div>`,
          );
          for (const field of oneofFields.entries()) {
            if (field[1].oneofdecl === declaration) {
              const hasType = field[1].type && field[1].type !== "";
              const hasLabel = field[1].label && field[1].label !== "";
              const hasDesc =
                field[1].description && field[1].description !== "";
              content.push(
                `<div className="${colHeaderStyle}">${field[1].name}</div>`,
              );
              content.push(`<div className="${colCellStyle}">`);
              if (hasType) {
                content.push(
                  `<div className="">[${field[1].type}](#${createId(field[1].fullType)})</div>`,
                );
              }
              if (hasLabel) {
                content.push(`<div className="">${field[1].label}</div>`);
              }
              if (hasDesc) {
                content.push(
                  `<div className="">${handleCurlies(field[1].description)
                    .replace(/\n\/?/g, " ")
                    .replace(/<(http.*)>/g, "$1")}</div>`,
                );
              }
              content.push("</div>");
            }
          }
        }
      }
      if (fields.length > 0 || oneofFields.length > 0) {
        content.push(`</div>`);
      }
    }
    if (file.services.length > 0) {
      const proto = file.name.split("/").pop();
      content.push(`<div className="pt-8"></div>`);
      content.push(`### Services (${proto})`);
      //content.push("<div className='ml-4'>");
      for (const service of file.services) {
        content.push(
          `<h4 className="" id="${createId(service.fullName)}">${service.name}</h4>`,
        );
        content.push(
          `<div>${handleCurlies(service.description)
            .replace(/\n\/?/g, " ")
            .replace(/<(http.*)>/g, "$1")}</div>`,
        );
        if (service.methods.length > 0) {
          content.push(`<div className='${tabStyle} mt-4'>`);
          content.push(`<div className='${tabHeaderStyle}'>Methods</div>`);
          for (const method of service.methods) {
            content.push(
              `<div className="${tabAltHeaderStyle}">[${method.requestType}](#${createId(method.requestFullType)}) -> ${method.responseType}</div>`,
            );
            content.push(
              `<div className="${tabRowStyle}">${handleCurlies(
                method.description,
              )
                .replace(/\n\/?/g, " ")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
          }
          content.push(`</div>`);
        }
      }
    }
    if (file.enums.length > 0) {
      const cellDesc = "mt-4";
      content.push(`<h4 className="mt-8">Enums</h4>`);
      for (const num of file.enums) {
        content.push(
          `<h4 className="mt-4" id="${createId(num.fullName)}">${num.name}</h4>`,
        );
        content.push(
          `<div class="${cellDesc}">${handleCurlies(num.description)
            .replace(/\n\/?/g, " ")
            .replace(/<(http.*)>/g, "$1")}</div>`,
        );
        content.push(`<div className='${tabStyle} mt-4'>`);
        content.push(`<div className='${tabHeaderStyle}'>Enums</div>`);

        //content.push(`<div>`);
        for (const val of num.values) {
          content.push(
            `<div className="${colHeaderStyle}"><code>${val.name}</code></div>`,
          );
          //
          content.push(
            `<div className="${colCellStyle}">${handleCurlies(val.description).replace(/\n+\/?/g, " ")
              .replace(/<(http.*)>/g, "$1")}</div>`,
          );
        }
        content.push(`</div>`);
      }
    }
    content.push("</div>");
  }
  content.push("\n## Scalar Value Types");
  const cellStyle =
    "m-2 min-w-24 max-w-[13rem] rounded-lg border border-solid align-center text-center relative border-sui-gray-65";
  const titleStyle =
    "p-4 pb-2 font-bold text-sui-ghost-dark dark:text-sui-ghost-white bg-sui-gray-50 dark:bg-sui-ghost-dark border border-solid border-transparent rounded-t-lg";
  const valStyle =
    "p-4 pt-2 border border-solid border-transparent border-t-sui-gray-65 whitespace-break-spaces";
  for (const scalar of spec.scalarValueTypes) {
    content.push(`\n### ${scalar.protoType}`);
    content.push(
      `<div className="text-lg">\n${handleCurlies(scalar.notes)}\n</div>`,
    );
    content.push(`<div className="flex flex-wrap">`);
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">C++</div><div className="${valStyle}">${scalar.cppType}</div></div>`,
    );
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">C#</div><div className="${valStyle}">${scalar.csType}</div></div>`,
    );
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">Go</div><div className="${valStyle}">${scalar.goType}</div></div>`,
    );
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">Java</div><div className="${valStyle}">${scalar.javaType}</div></div>`,
    );
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">PHP</div><div className="${valStyle}">${scalar.phpType}</div></div>`,
    );
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">Python</div><div className="${valStyle}">${scalar.pythonType}</div></div>`,
    );
    content.push(
      `<div className="${cellStyle}"><div className="${titleStyle}">Ruby</div><div className="${valStyle}">${scalar.rubyType}</div></div>`,
    );
    content.push(`</div>`);
  }

  return (
    callback &&
    callback(null, source.replace(/<Protocol ?\/>/, content.join(`\n`)))
  );
};

module.exports = protocolInject;
