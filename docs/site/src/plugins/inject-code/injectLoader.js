// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require("fs");
const path = require("path");

const addCodeInject = function (source) {
  let fileString = source;
  const callback = this.async();

  const options = this.getOptions();
  const markdownFilename = path.basename(this.resourcePath);
  const repoPath = path.join(__dirname, "../../../../..");

  // Do not load and render markdown files without docusaurus header.
  // These files are only used to be included in other files and should not generate their own web page
  if (fileString.length >= 3 && fileString.substring(0, 3) !== "---") {
    return callback && callback(null, "");
  }

  function addMarkdownIncludes(fileContent) {
    let res = fileContent;
    const matches = fileContent.match(/\{@\w+: .+\}/g);
    if (matches) {
      matches.forEach((match) => {
        const replacer = new RegExp(match, "g");
        const key = "{@inject: ";
        // {@inject: code.move#function=sword_create}
        if (match.startsWith(key)) {
          const injectFileFull = match.substring(key.length, match.length - 1);
          const injectFile = injectFileFull.substring(
            0,
            injectFileFull.indexOf("#") > 0
              ? injectFileFull.indexOf("#")
              : injectFileFull.length,
          );
          let fileExt = injectFile.substring(injectFile.lastIndexOf(".") + 1);
          let language = "";
          const fullPath = path.join(repoPath, injectFile);

          // Assuming rust manifests and locks never being used
          switch (fileExt) {
            case "toml":
              language = "move";
              break;
            case "lock":
              language = "move";
              break;
            case "sh":
              language = "shell";
              break;
            case "mdx":
              language = "markdown";
              break;
            case "tsx":
              language = "ts";
              break;
            default:
              language = fileExt;
          }

          if (fs.existsSync(fullPath)) {
            let injectFileContent = fs.readFileSync(fullPath, "utf8");
            const marker =
              injectFileFull.indexOf("#") > 0
                ? injectFileFull.substring(injectFileFull.indexOf("#"))
                : null;

            if (marker) {
              const checkBracesBalance = (str) => {
                const openBraces = str.match(/{/g) || [];
                const closeBraces = str.match(/}/g) || [];

                return openBraces.length === closeBraces.length;
              };
              //const removeLeadingSpaces = (str, numSpaces) => {

              const removeLeadingSpaces = (matchArray, prepend = "") => {
                const numSpaces = matchArray[1] ? matchArray[1].length - 1 : 0;
                if (numSpaces === 0) {
                  return prepend + matchArray[0];
                }
                const lines = matchArray[0].split("\n");

                return [
                  prepend,
                  lines.map((line) => line.substring(numSpaces)).join("\n"),
                ].join("\n");
              };
              const funKey = "#function=";
              const structKey = "#struct=";
              const funName =
                marker.indexOf(funKey) >= 0
                  ? marker
                      .substring(marker.indexOf(funKey) + funKey.length)
                      .trim()
                  : null;
              const structName =
                marker.indexOf(structKey) >= 0
                  ? marker
                      .substring(marker.indexOf(structKey) + structKey.length)
                      .trim()
                  : null;
              if (funName) {
                const funStr = `^(\\s*)*(public )?fun ${funName}\\b(?=[^\\w]).*?}\\n(?=\\n)`;
                const funRE = new RegExp(funStr, "ms");
                const funMatch = funRE.exec(injectFileContent);
                if (funMatch) {
                  const numSpaces = funMatch[1] ? funMatch[1].length - 1 : 0;
                  let preFun = injectFileContent.substring(
                    0,
                    funMatch.index - 1,
                  );
                  const lines = preFun.split("\n");
                  let pre = [];
                  for (let x = lines.length - 1; x > 0; x--) {
                    if (lines[x].trim() === "}") {
                      break;
                    } else {
                      pre.push(lines[x].substring(numSpaces));
                    }
                  }
                  preFun = pre.reverse().join("\n");
                  if (!checkBracesBalance(injectFileContent)) {
                    injectFileContent =
                      "Could not find valid function definition. If code is formatted correctly, consider using code comments instead.";
                  } else {
                    injectFileContent = removeLeadingSpaces(funMatch, preFun);
                  }
                }
              } else if (structName) {
                const structStr = `^(\\s*)*(public )?struct \\b${structName}(?=[^\\w]).*?}\\n(?=\\n)`;
                const structRE = new RegExp(structStr, "ms");
                const structMatch = structRE.exec(injectFileContent);
                if (structMatch) {
                  if (!checkBracesBalance(structMatch[0])) {
                    injectFileContent =
                      "Could not find valid struct definition. If code is formatted correctly, consider using code comments instead";
                  } else {
                    injectFileContent = removeLeadingSpaces(structMatch);
                  }
                } else {
                  injectFileContent = "Struct not found.";
                }
              } else {
                const regexStr = `\\/\\/\\s?docs::${marker.trim()}\\b([\\s\\S]*)\\/\\/\\s*docs::\\/\\s?${marker.trim()}\\b`;
                const closingsStr = `\\/\\/\\s?docs::\\/${marker.trim()}\\b([)};]*)`;

                const closingRE = new RegExp(closingsStr, "g");

                const regex = new RegExp(regexStr, "g");
                const match = regex.exec(injectFileContent);
                const closingStr = closingRE.exec(injectFileContent);
                if (match) {
                  injectFileContent = match[1];
                }
                if (closingStr) {
                  const closingTotal = closingStr[1].length;
                  let closingArray = [];
                  for (let i = 0; i < closingTotal; i++) {
                    const currentChar = closingStr[1][i];
                    const nextChar = closingStr[1][i + 1];

                    if (nextChar === ";") {
                      closingArray.push(currentChar + nextChar);
                      i++;
                    } else {
                      closingArray.push(currentChar);
                    }
                  }
                  const totClosings = closingArray.length;

                  // Process any closing elements added in the closing comment of source code
                  let closing = "";
                  for (let j = 0; j < totClosings; j++) {
                    let space = "  ".repeat(totClosings - 1 - j);
                    closing += `\n${space}${closingArray[j]}`;
                  }
                  injectFileContent = injectFileContent.trim() + closing;
                }

                // Start searching for the pause doc comments
                // Must be in form: // docs::#idName-pause: optional replacement text
                // Must be followed at some point by: // docs::#idName-resume
                // Replace text in the middle with optional replacement text or blankness
                const pauseStr = `\\/\\/\\s?docs::${marker.trim()}-pause:?(.*)`;
                const pauseRE = new RegExp(pauseStr, "g");
                let matches = injectFileContent.match(pauseRE);
                for (let match in matches) {
                  const test = matches[match].replace(
                    /[.*+?^${}()|[\]\\]/g,
                    "\\$&",
                  );
                  let replacer = "";
                  if (matches[match].indexOf("-pause:") > 0) {
                    replacer = matches[match].substring(
                      matches[match].indexOf("-pause") + 8,
                    );
                  }

                  const newRE = new RegExp(test);
                  const resumeRE = new RegExp(
                    `\\/\\/\\s?docs::${marker.trim()}-resume`,
                  );
                  let paused;
                  if (replacer !== "") {
                    paused = new RegExp(
                      newRE.source.replace(":?(.*)", "") +
                        ".*?" +
                        resumeRE.source,
                      "gs",
                    );
                  } else {
                    paused = new RegExp(
                      newRE.source.replace(":?(.*)", "") +
                        "(?!:).*?" +
                        resumeRE.source,
                      "gs",
                    );
                  }
                  injectFileContent = injectFileContent.replace(
                    paused,
                    replacer,
                  );
                }
              }

              // Replace all the //docs:: lines in code
              injectFileContent = injectFileContent.replace(
                /^\s*\/\/\s*docs::\/?.*\r?$\n?/gm,
                "",
              );

              const trimContent = (content) => {
                let arr = content.split("\n");
                let start = 0;
                let end = arr.length;

                while (start < end && arr[start] === "") {
                  start++;
                }

                while (end > start && arr[end - 1] === "") {
                  end--;
                }

                return arr.slice(start, end).join("\n");
              };

              injectFileContent = trimContent(injectFileContent);
              injectFileContent = `\`\`\`${language} title=${injectFile}\n${injectFileContent}\n\`\`\``;

              res = res.replace(replacer, injectFileContent);
              res = addMarkdownIncludes(res);
            }
          } else {
            res = res.replace(
              replacer,
              `\n> Code to inject not found: ${injectFile} --> ${fullPath}\n`,
            );
          }
        } else {
          const parts = match.substring(2, match.length - 3).split(": ");
          if (parts.length === 2) {
            if (options.embeds) {
              for (const embed of options.embeds) {
                if (embed.key === parts[0]) {
                  const embedResult = embed.embedFunction(parts[1]);
                  res = res.replace(replacer, embedResult);
                }
              }
            }
          }
        }
      });
    }
    return res;
  }

  function replacePlaceHolders(documentContent) {
    var res = documentContent;
    if (options.replacements) {
      var placeHolders = [...options.replacements];
      if (!placeHolders) {
        placeHolders = [];
      }
      placeHolders.push({
        key: "{ContainerMarkdown}",
        value: markdownFilename,
      });
      placeHolders.forEach((replacement) => {
        const replacer = new RegExp(replacement.key, "g");
        res = res.replace(replacer, replacement.value);
      });
    }
    return res;
  }

  fileString = replacePlaceHolders(addMarkdownIncludes(fileString));

  return callback && callback(null, fileString);
};

module.exports = addCodeInject;
