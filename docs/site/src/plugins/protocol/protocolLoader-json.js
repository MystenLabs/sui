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
    let item = { name: proto.name, link: protoLink, messages: messages };
    toc.push(item);
  }
  const types = [];
  for (const prototype of spec.scalarValueTypes) {
    types.push({ name: prototype.protoType, link: prototype.protoType });
  }
  toc.push({
    name: "Scalar Value Types",
    link: "scalar-value-types",
    messages: types,
  });

  let content = [`<Protocol toc={${JSON.stringify(toc)}}/>`];

  let messageSort = (array) => {
    return array.sort((a, b) => a.name.localeCompare(b.name));
  };

  for (const file of spec.files) {
    content.push(`\n## ${file.name} {#${createId(file.name)}}`);
    content.push(
      `${file.description.replace(/{/g, "&#123;").replace(/^##? (.*)\n/, "### $1")}`,
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
        `${message.description
          .replace(/{/g, "&#123;")
          .replace(/</g, "&#lt;")
          .replace(/^(#{1,2})\s(?!#)/gm, "### ")}`,
      );

      if (allFields.length > 0) {
        const attrStyle =
          " text-xl before:pr-2 before:mr-2 before:text-sm before:border before:border-solid before:border-transparent before:border-r-sui-gray-65";
        const fieldStyle =
          "p-2 font-medium text-xl rounded-lg bg-sui-ghost-white dark:bg-sui-ghost-dark";
        const borderStyle =
          "border border-solid border-y-transparent border-r-transparent border-l-sui-gray-65";
        const leftArrowStyle =
          "relative inline-flex items-center before:content-[''] before:border-t-transparent before:border-b-transparent before:border-solid before:border-y-5 before:border-r-0 before:border-l-8 before:border-l-sui-gray-65";
        content.push(`<p class="ml-4 text-2xl">Fields</p>`);
        content.push(`<div class="ml-4">`);
        content.push(`<div class="grid grid-cols-12">`);
        let foundoneof = false;
        for (const [idx, field] of allFields.entries()) {
          const hasType = field.type && field.type !== "";
          const hasLabel = field.label && field.label !== "";
          const hasDesc = field.description && field.description !== "";
          if (field.isoneof) {
            if (!foundoneof) {
              content.push(
                `<div class="col-span-3 ${leftArrowStyle} ${borderStyle}"><div class="${fieldStyle} py-2">One of</div></div>`,
              );
              foundoneof = !foundoneof;
            }
          }
          content.push(
            `<div class="${field.isoneof ? "col-start-2 col-end-12" : "col-span-12"} ${borderStyle} py-2">`,
          );
          content.push(`<div class="${leftArrowStyle}">`);
          content.push(
            `<div class="${fieldStyle} col-span-12">${field.name}</div>\n</div>`,
          );
          content.push(`<div class="flex flex-row ml-4 pt-2 items-center">`);
          if (hasType) {
            content.push(
              `<div class="${attrStyle} before:content-['Type']">[${field.type}](#${createId(field.fullType)})</div>`,
            );
          }
          if (hasLabel) {
            content.push(
              `<div class="${attrStyle} before:content-['Label'] ml-4">${field.label}</div>`,
            );
          }
          content.push("</div>");
          if (hasDesc) {
            content.push(`<div class="ml-4 pt-2">`);
            content.push(
              `<div class="${attrStyle} before:content-['Description']">${field.description
                .replace(/{/g, "&#123;")
                .replace(/\n\/?/g, "")
                .replace(/<(http.*)>/g, "$1")}</div>`,
            );
            content.push(`</div>`);
          }
          content.push("</div>");
        }

        content.push("</div>\n</div>");
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
    content.push(`${scalar.notes.replace(/{/g, "&#123;")}`);
    content.push(`<div class="flex flex-wrap">`);
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">C++</div><div class="${valStyle}">${scalar.cppType}</div></div>`,
    );
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">C#</div><div class="${valStyle}">${scalar.csType}</div></div>`,
    );
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">Go</div><div class="${valStyle}">${scalar.goType}</div></div>`,
    );
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">Java</div><div class="${valStyle}">${scalar.javaType}</div></div>`,
    );
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">PHP</div><div class="${valStyle}">${scalar.phpType}</div></div>`,
    );
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">Python</div><div class="${valStyle}">${scalar.pythonType}</div></div>`,
    );
    content.push(
      `<div class="${cellStyle}"><div class="${titleStyle}">Ruby</div><div class="${valStyle}">${scalar.rubyType}</div></div>`,
    );
    content.push(`</div>`);
  }
  //console.log(content.join(`\n`))
  return (
    callback &&
    callback(null, source.replace(/<Protocol ?\/>/, content.join(`\n`)))
  );
};

module.exports = protocolInject;
