// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require("fs");
const path = require("path");
const utils = require("./utils.js");

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
    const matches = fileContent.match(/(?<!`)\{@\w+: .+\}/g);
    if (matches) {
      matches.forEach((match) => {
        const replacer = new RegExp(match, "g");
        const key = "{@inject: ";

        if (match.startsWith(key)) {
          const parts = match.split(" ");
          const [, , ...options] = parts.length > 2 ? parts : [];
          let injectFileFull = parts[1].replace(/\}$/, "");

          const injectFile = injectFileFull.split("#")[0];

          let fileExt = injectFile.substring(injectFile.lastIndexOf(".") + 1);
          let language = "";
          const fullPath = path.join(repoPath, injectFile);

          switch (fileExt) {
            case "lock":
              language = "toml";
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
              const funKey = "#fun=";
              const structKey = "#struct=";
              const moduleKey = "#module=";
              const getName = (mark, key) => {
                return mark.indexOf(key, mark) >= 0
                  ? mark.substring(mark.indexOf(key) + key.length).trim()
                  : null;
              };
              const funName = getName(marker, funKey);
              const structName = getName(marker, structKey);
              const moduleName = getName(marker, moduleKey);
              if (funName) {
                const funs = funName.split(",");
                let funContent = [];
                for (let fn of funs) {
                  fn = fn.trim();
                  const funStr = `^(\\s*)*?(public )?fun \\b${fn}\\b.*?}\\n(\\s*?})?(?=\\n)?`;
                  const funRE = new RegExp(funStr, "msi");
                  const funMatch = funRE.exec(injectFileContent);
                  if (funMatch) {
                    let preFun = utils.capturePrepend(
                      funMatch,
                      injectFileContent,
                    );
                    // Check if last function in module, removing last } if true.
                    if (
                      funMatch[0].match(/}\s*}\s*$/s) &&
                      !utils.checkBracesBalance(funMatch[0])
                    ) {
                      funContent.push(
                        utils.removeLeadingSpaces(
                          funMatch[0].replace(/}$/, ""),
                          preFun,
                        ),
                      );
                    } else {
                      funContent.push(
                        utils.removeLeadingSpaces(funMatch[0], preFun),
                      );
                    }
                  }
                }
                injectFileContent = funContent
                  .join("\n")
                  .replace(/\n{3}/gm, "\n\n")
                  .trim();
              } else if (structName) {
                const structs = structName.split(",");
                let structContent = [];
                for (let struct of structs) {
                  struct = struct.trim();
                  const structStr = `^(\\s*)*?(public )?struct \\b${struct}\\b.*?}`;
                  const structRE = new RegExp(structStr, "msi");
                  const structMatch = structRE.exec(injectFileContent);
                  if (structMatch) {
                    let preStruct = utils.capturePrepend(
                      structMatch,
                      injectFileContent,
                    );
                    structContent.push(
                      utils.removeLeadingSpaces(structMatch[0], preStruct),
                    );
                  } else {
                    injectFileContent =
                      "Struct not found. If code is formatted correctly, consider using code comments instead.";
                  }
                }
                injectFileContent = structContent.join("\n").trim();
              } else if (moduleName) {
                const modStr = `^(\\s*)*module \\b${moduleName}\\b.*?}\\n(?=\\n)?`;
                const modRE = new RegExp(modStr, "msi");
                const modMatch = modRE.exec(injectFileContent);
                if (modMatch) {
                  const abridged = injectFileContent.substring(modMatch.index);
                  const lines = abridged.split("\n");
                  let open = [];
                  let close = [];
                  let modLines = [];
                  for (let line of lines) {
                    modLines.push(line);
                    open = [...open, ...(line.match(/{/g) || [])];
                    close = [...close, ...(line.match(/}/g) || [])];
                    if (open.length !== 0 && close.length === open.length) {
                      break;
                    }
                  }
                  const preMod = utils.capturePrepend(
                    modMatch,
                    injectFileContent,
                  );
                  injectFileContent = utils.removeLeadingSpaces(
                    modLines.join("\n"),
                    preMod,
                  );
                } else {
                  injectFileContent =
                    "Module not found. If code is formatted correctly, consider using code comments instead.";
                }
              } else {
                const regexStr = `\\/\\/\\s?docs::${marker.trim()}\\b([\\s\\S]*)\\/\\/\\s*docs::\\/\\s?${marker.trim()}\\b`;
                const closingsStr = `\\/\\/\\s?docs::\\/${marker.trim()}\\b([)};]*)`;

                const closingRE = new RegExp(closingsStr, "g");

                const regex = new RegExp(regexStr, "g");
                const match = regex.exec(injectFileContent);
                const closingStr = closingRE.exec(injectFileContent);
                var closing = "";
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
                  for (let j = 0; j < totClosings; j++) {
                    let space = "  ".repeat(totClosings - 1 - j);
                    closing += `\n${space}${closingArray[j]}`;
                  }
                }

                // Start searching for the pause doc comments
                // Must be in form: // docs::#idName-pause: optional replacement text
                // Must be followed at some point by: // docs::#idName-resume
                // Replace text in the middle with optional replacement text or blankness
                const pauseStr = `\\/\\/\\s?docs::${marker.trim()}-pause:?(.*)`;
                const pauseRE = new RegExp(pauseStr, "g");
                let matches = injectFileContent.match(pauseRE);
                if (matches) {
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
                injectFileContent =
                  utils.removeLeadingSpaces(injectFileContent) + closing;
              }

              injectFileContent = utils.processOptions(
                injectFileContent,
                options,
              );

              injectFileContent = utils.formatOutput(
                language,
                injectFile,
                injectFileContent,
              );
              res = res.replace(replacer, injectFileContent);
              res = addMarkdownIncludes(res);
            } else {
              // Handle import of all the code
              const processed = utils.processOptions(
                injectFileContent,
                options,
              );
              const processedFileContent = utils.formatOutput(
                language,
                injectFile,
                processed,
              );
              res = res.replace(replacer, processedFileContent);
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
