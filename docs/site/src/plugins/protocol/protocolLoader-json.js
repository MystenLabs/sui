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
  const leftArrowStyle =
    "relative inline-flex items-center before:content-[''] before:border-t-transparent before:border-b-transparent before:border-solid before:border-y-5 before:border-r-0 before:border-l-8 before:border-l-sui-gray-65";
  const borderStyle =
    "border border-solid border-y-transparent border-r-transparent border-l-sui-gray-65";
  const attrStyle =
    "text-lg before:pr-2 before:mr-2 before:text-sm before:border before:border-solid before:border-transparent before:border-r-sui-gray-65";
  const fieldStyle =
    "p-2 font-medium text-lg rounded-lg bg-sui-ghost-white dark:bg-sui-ghost-dark";
  for (const file of files) {
    content.push(`\n## ${file.name} {#${createId(file.name)}}`);
    content.push(
      `<div className="text-lg">\n${handleCurlies(file.description).replace(
        /\n##? (.*)\n/,
        "### $1",
      )}\n</div>`,
    );
    for (const message of file.messages) {
      let fields = [];
      let oneofFields = [];
      if (message.hasOneofs) {
        for (const field of message.fields) {
          if (field.isoneof === true) {
            oneofFields.push(field);
          } else {
            fields.push(field);
          }
        }
      } else {
        fields = Object.values(message.fields);
      }

      const allFields = [...messageSort(fields), ...messageSort(oneofFields)];
      content.push(`\n### ${message.name} {#${createId(message.fullName)}}`);
      content.push(
        `<div className="text-lg">\n${handleCurlies(message.description)
          .replace(/</g, "&#lt;")
          .replace(/^(#{1,2})\s(?!#)/gm, "### ")}\n
        </div>`,
      );

      if (allFields.length > 0) {
        content.push(`<p className="ml-4 text-2xl">Fields</p>`);
        content.push(`<div className="ml-4">`);
        content.push(`<div className="grid grid-cols-12">`);
        let foundoneof = false;
        for (const [idx, field] of allFields.entries()) {
          const hasType = field.type && field.type !== "";
          const hasLabel = field.label && field.label !== "";
          const hasDesc = field.description && field.description !== "";
          if (field.isoneof) {
            if (!foundoneof) {
              content.push(
                `<div className="col-span-3 ${leftArrowStyle} ${borderStyle}"><div className="${fieldStyle} py-2">One of</div></div>`,
              );
              foundoneof = !foundoneof;
            }
          }
          content.push(
            `<div className="${field.isoneof ? "col-start-2 col-end-12" : "col-span-12"} ${borderStyle} py-2">`,
          );
          content.push(`<div className="${leftArrowStyle}">`);
          content.push(
            `<div className="${fieldStyle} col-span-12">${field.name}</div>\n</div>`,
          );
          content.push(
            `<div className="flex flex-row ml-4 pt-2 items-center">`,
          );
          if (hasType) {
            content.push(
              `<div className="${attrStyle} before:content-['Type']">[${field.type}](#${createId(field.fullType)})</div>`,
            );
          }
          if (hasLabel) {
            content.push(
              `<div className="${attrStyle} before:content-['Label'] ml-4">${field.label}</div>`,
            );
          }
          content.push("</div>");
          if (hasDesc) {
            content.push(`<div className="ml-4 pt-2">`);
            content.push(
              `<div className="${attrStyle} before:content-['Description'] indent-[-88px] pl-[88px]">${handleCurlies(
                field.description,
              )
                .replace(/\n\/?/g, " ")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
            content.push(`</div>`);
          }
          content.push("</div>");
        }

        content.push("</div>\n</div>");
      }
    }
    if (file.services.length > 0) {
      const proto = file.name.split("/").pop();
      content.push(`### Services (${proto})`);
      content.push("<div className='ml-4'>");
      for (const service of file.services) {
        content.push(`<div className="${borderStyle}">`);
        content.push(
          `<h4 className="${leftArrowStyle} before:mr-2" id="${createId(service.fullName)}">${service.name}</h4>`,
        );
        content.push(`<div className="ml-4">`);
        content.push(
          `<div>${handleCurlies(service.description)
            .replace(/\n\/?/g, " ")
            .replace(/<(http.*)>/g, "$1")}</div>`,
        );
        if (service.methods.length > 0) {
          content.push(`<p className="text-xl mt-4">Methods</p>`);
          for (const method of service.methods) {
            content.push(`<div className="my-6">`);
            content.push(
              `<div className="${attrStyle} my-2 before:content-['Request']  before:inline-block before:w-20">[${method.requestType}](#${createId(method.requestFullType)})</div>`,
            );
            content.push(
              `<div className="${attrStyle} my-2 before:content-['Response'] before:inline-block before:w-20">${method.responseType}</div>`,
            );
            content.push(
              `<div className="${attrStyle} my-2 table relative left-2 before:table-cell before:content-['Description'] before:relative before:-left-2 before:w-20">${handleCurlies(
                method.description,
              )
                .replace(/\n\/?/g, " ")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
            content.push(`</div>`);
          }
        }
        content.push("</div></div>");
        //content.push();
        //content.push();
      }
      content.push("</div>");
    }
    if (file.enums.length > 0) {
      content.push("<div className='ml-4'>");
      content.push("### Enums");
      for (const num of file.enums) {
        content.push(`<div className="${borderStyle}">`);
        content.push(
          `<h4 className="${leftArrowStyle} before:mr-2" id="${createId(num.fullName)}">${num.name}</h4>`,
        );
        content.push(`<div className="ml-4">`);
        content.push(
          `<div className="${attrStyle} before:content-['Description'] indent-[-88px] pl-[88px]">${handleCurlies(
            num.description,
          )
            .replace(/\n\/?/g, " ")
            .replace(/<(http.*)>/g, "$1")}</div>`,
        );
        content.push(
          `<div className="mt-4 flex flex-row ${attrStyle} before:!mr-0 before:content-['Values']">`,
        );
        content.push(`<div>`);
        for (const val of num.values) {
          content.push(
            `<div class="flex flex-row my-2"><div className="${leftArrowStyle} before:mr-2"><code>${val.name}</code></div><div>${val.description ? ": " + handleCurlies(val.description) : ""}</div></div>`,
          );
        }
        content.push(`</div></div></div></div></div>`);
      }
    }
  }

  content.push("\n## Scalar Value Types");
  const cellStyle =
    "m-2 min-w-24 max-w-[13rem] rounded-lg border border-solid align-center text-center relative border-sui-gray-65";
  const titleStyle =
    "p-4 pb-2 font-bold text-sui-ghost-dark dark:text-sui-ghost-white bg-sui-ghost-white dark:bg-sui-ghost-dark border border-solid border-transparent rounded-t-lg";
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
