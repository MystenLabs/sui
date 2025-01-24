// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const fs = require("fs");
const path = require("path");
const https = require("https");
const utils = require("./utils.js");

const GITHUB = "https://raw.githubusercontent.com/";
const GITHUB_RAW = "refs/heads/main/";

const addCodeInject = async function (source) {
  let fileString = source;
  const callback = this.async();
  const options = this.getOptions();

  const markdownFilename = path.basename(this.resourcePath);

  // Do not load and render markdown files without docusaurus header.
  // These files are only used to be included in other files and should not generate their own web page
  if (fileString.length >= 3 && fileString.substring(0, 3) !== "---") {
    return callback && callback(null, "");
  }

  const fetchFile = (url) => {
    return new Promise((res, rej) => {
      let data = "";
      https
        .get(url, (response) => {
          if (response.statusCode !== 200) {
            console.error(
              `Failed to fetch GitHub data: ${response.statusCode}`,
            );
            res("Error loading content");
          }

          response.on("data", (chunk) => {
            data += chunk;
          });

          response.on("end", () => {
            res(data);
          });
        })
        .on("error", (err) => {
          rej(`Error: ${err.message}`);
        });
    });
  };

  const pathBuild = (srcPath) => {
    const parts = srcPath.split("/");
    if (parts[0].includes("github")) {
      const githubOrgName = parts[0].split(":")[1];
      const githubRepoName = parts[1];
      return path.join(
        GITHUB,
        githubOrgName,
        githubRepoName,
        GITHUB_RAW,
        parts.slice(2).join("/"),
      );
    } else {
      return path.join(__dirname, "../../../../..", srcPath);
    }
  };

  async function addMarkdownIncludes(fileContent) {
    let res = fileContent;
    const matches = fileContent.match(/(?<!`)\{@\w+: .+\}/g);
    if (matches) {
      for (const match of matches) {
        const replacer = new RegExp(match, "g");
        const key = "{@inject: ";

        if (match.startsWith(key)) {
          const parts = match.split(" ");
          const [, , ...options] = parts.length > 2 ? parts : [];
          let injectFileFull = parts[1].replace(/\}$/, "");
          let injectFile = injectFileFull.split("#")[0];
          let fileExt = injectFile.substring(injectFile.lastIndexOf(".") + 1);
          let language = "";
          const fullPath = pathBuild(injectFile);

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
            case "rs":
              language = "rust";
              break;
            case "prisma":
              language = "ts";
              break;
            default:
              language = fileExt;
          }

          const isMove = language === "move";
          const isTs = language === "ts" || language === "js";

          if (fs.existsSync(fullPath) || fullPath.match(/^https/)) {
            let injectFileContent;
            if (fullPath.match(/^https/)) {
              injectFileContent = await fetchFile(fullPath);
            } else {
              injectFileContent = fs
                .readFileSync(fullPath, "utf8")
                .replaceAll(`\t`, "  ");
            }

            const marker =
              injectFileFull.indexOf("#") > 0
                ? injectFileFull.substring(injectFileFull.indexOf("#"))
                : null;

            if (marker) {
              const funKey = "#fun=";
              const structKey = "#struct=";
              const moduleKey = "#module=";
              const varKey = "#variable=";
              const useKey = "#use=";
              const componentKey = "#component=";
              const enumKey = "#enum=";
              const typeKey = "#type=";
              const getName = (mark, key) => {
                return mark.indexOf(key, mark) >= 0
                  ? mark.substring(mark.indexOf(key) + key.length).trim()
                  : null;
              };
              const funName = getName(marker, funKey);
              const structName = getName(marker, structKey);
              const moduleName = getName(marker, moduleKey);
              const variableName = getName(marker, varKey);
              const useName = getName(marker, useKey);
              const componentName = getName(marker, componentKey);
              const enumName = getName(marker, enumKey);
              const typeName = getName(marker, typeKey);
              if (funName) {
                const funs = funName.split(",");
                let funContent = [];

                for (let fn of funs) {
                  fn = fn.trim();
                  let funStr = "";
                  if (isMove) {
                    funStr = `^(\\s*)*?(pub(lic)? )?(entry )?fu?n \\b${fn}\\b.*?}\\n(\\s*?})?(?=\\n)?`;
                  } else if (isTs) {
                    funStr = `^(\\s*)(async )?(export (default )?)?function \\b${fn}\\b.*?\\n\\1}\\n`;
                  }
                  const funRE = new RegExp(funStr, "msi");
                  const funMatch = funRE.exec(injectFileContent);
                  if (funMatch) {
                    let preFun = utils.capturePrepend(
                      funMatch,
                      injectFileContent,
                    );
                    // Check if last function in module, removing last } if true.
                    if (
                      isMove &&
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
                  const structStr = `^(\\s*)*?(pub(lic)? )?struct \\b${struct}\\b.*?}`;
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
              } else if (variableName) {
                const vs = variableName.split(",");
                let temp = "";
                let isGroup = false;
                let groupedVars = [];
                vs.forEach((v) => {
                  if (v.startsWith("(")) {
                    temp = v;
                    isGroup = true;
                  } else if (isGroup) {
                    temp += ", " + v;
                    if (temp.endsWith(")")) {
                      groupedVars.push(temp);
                      temp = "";
                      isGroup = false;
                    }
                  } else {
                    groupedVars.push(v);
                  }
                });
                let varContent = [];
                if (language === "ts" || language === "js") {
                  const varTsFunction = `^( *)?.*?(let|const) \\b${variableName}\\b.*=>`;
                  const varTsVariable = `^( *)?.*?(let|const) \\b${variableName}\\b (?!.*=>)=.*;`;
                  const varTsRE = new RegExp(varTsFunction, "m");
                  const varTsVarRE = new RegExp(varTsVariable, "m");
                  const varTsMatch = varTsRE.exec(injectFileContent);
                  const varTsVarMatch = varTsVarRE.exec(injectFileContent);
                  if (varTsMatch) {
                    const start = injectFileContent.slice(varTsMatch.index);
                    const endText = `^${varTsMatch[1] ? varTsMatch[1] : ""}\\)?\\};`;
                    const endRE = new RegExp(endText, "m");
                    const endMatch = endRE.exec(start);
                    let preVarTs = utils.capturePrepend(
                      varTsMatch,
                      injectFileContent,
                    );
                    varContent.push(
                      utils.removeLeadingSpaces(
                        start.slice(0, endMatch.index + endMatch[0].length),
                        preVarTs,
                      ),
                    );
                  } else if (varTsVarMatch) {
                    let preVarTs2 = utils.capturePrepend(
                      varTsVarMatch,
                      injectFileContent,
                    );
                    varContent.push(
                      utils.removeLeadingSpaces(varTsVarMatch[0], preVarTs2),
                    );
                  }
                } else {
                  for (let v of groupedVars) {
                    v = v.trim();
                    const varStrShort = `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${v}\\b.*?\\)?\\s?=.*;`;
                    //const varStrLong = `^(\\s*)?(#\\[test_only\\])?(let|const) ${v}.*\\{.*\\};\\n`;
                    const varStrLong = `^(\\s*)?(#\\[test_only\\])?(let|const) \\(?.*?\\b${v}\\b.*?\\)?\\s?= \\{[^}]*\\};\\n`;
                    const varREShort = new RegExp(varStrShort, "m");
                    const varRELong = new RegExp(varStrLong, "m");
                    const varShortMatch = varREShort.exec(injectFileContent);
                    const varLongMatch = varRELong.exec(injectFileContent);
                    if (varShortMatch || varLongMatch) {
                      let varMatch = varShortMatch
                        ? varShortMatch
                        : varLongMatch;
                      let preVar = utils.capturePrepend(
                        varMatch,
                        injectFileContent,
                      );
                      varContent.push(
                        utils.removeLeadingSpaces(varMatch[0], preVar),
                      );
                    } else {
                      injectFileContent =
                        "Variable not found. If code is formatted correctly, consider using code comments instead.";
                    }
                  }
                }

                injectFileContent = varContent.join("\n").trim();
              } else if (useName) {
                const us = useName.split(",");
                let useContent = [];
                for (let u of us) {
                  u = u.trim();
                  const uArray = u.split("::");
                  const useStr = `^( *)(#\\[test_only\\] )?use ${uArray[0]}::\\{?.*?${uArray[1] ? uArray[1] : ""}.*?\\};`;
                  const useRE = new RegExp(useStr, "ms");
                  const useMatch = useRE.exec(injectFileContent);
                  if (useMatch) {
                    let preUse = utils.capturePrepend(
                      useMatch,
                      injectFileContent,
                    );
                    useContent.push(
                      utils.removeLeadingSpaces(useMatch[0], preUse),
                    );
                  } else {
                    injectFileContent =
                      "Use statement not found. If code is formatted correctly, consider using code comments instead.";
                  }
                }

                injectFileContent = useContent.join("\n").trim();
              } else if (componentName) {
                const components = componentName.split(",");
                let componentContent = [];
                for (let comp of components) {
                  let names = [];
                  let name = comp;
                  let element = "";
                  let ordinal = "";
                  if (comp.indexOf(":") > 0) {
                    names = comp.split(":");
                    name = names[0];
                    element = names[1];
                    ordinal = names[2] ? names[2] : "";
                  }
                  const compStr = `^( *)(export (default )?)?function \\b${name}\\b.*?\\n\\1\\}\\n`;
                  const compRE = new RegExp(compStr, "ms");
                  const compMatch = compRE.exec(injectFileContent);
                  if (compMatch) {
                    if (element) {
                      const elStr = `^( *)\\<${element}\\b.*?>.*?\\<\\/${element}>`;
                      const elRE = new RegExp(elStr, "msg");
                      let elementsToKeep = [1];
                      if (ordinal) {
                        if (
                          ordinal.indexOf("-") > 0 &&
                          ordinal.indexOf("&") > 0
                        ) {
                          console.log(
                            "Only dashes or commas allowed for selecting component elements, not both.",
                          );
                        } else {
                          if (ordinal.indexOf("-") > 0) {
                            const [start, end] = ordinal.split("-").map(Number);
                            elementsToKeep = Array.from(
                              { length: end - start + 1 },
                              (_, i) => start + i,
                            );
                          }
                          if (ordinal.indexOf("&") > 0) {
                            elementsToKeep = ordinal.split("&").map(Number);
                          }
                        }
                      }
                      elementsToKeep.sort((a, b) => a - b);
                      for (
                        let x = 0;
                        x < elementsToKeep[elementsToKeep.length - 1];
                        x++
                      ) {
                        const elMatch = elRE.exec(compMatch);
                        if (elementsToKeep.includes(x + 1)) {
                          componentContent.push(
                            utils.removeLeadingSpaces(elMatch[0]),
                          );
                        } else {
                          if (
                            x > 0 &&
                            componentContent[x - 1].trim() !== "..."
                          ) {
                            componentContent.push("\n...");
                          }
                        }
                      }
                    } else {
                      let preComp = utils.capturePrepend(
                        compMatch,
                        injectFileContent,
                      );
                      componentContent.push(
                        utils.removeLeadingSpaces(compMatch[0], preComp),
                      );
                    }
                  }
                }
                injectFileContent = componentContent.join("\n").trim();
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
              } else if (enumName) {
                const enums = enumName.split(",");
                let enumContent = [];
                for (let e of enums) {
                  const enumStr = `^( *)(export)? enum \\b${e}\\b\\s*\\{[^}]*\\}`;
                  const enumRE = new RegExp(enumStr, "m");
                  const enumMatch = enumRE.exec(injectFileContent);
                  if (enumMatch) {
                    enumContent.push(utils.removeLeadingSpaces(enumMatch[0]));
                  }
                }
                injectFileContent = enumContent.join("\n").trim();
              } else if (typeName) {
                const types = typeName.split(",");
                let typeContent = [];
                for (let t of types) {
                  const typeStartStr = `^( *)(export )?type \\b${t}\\b`;
                  const typeRE = new RegExp(typeStartStr, "m");
                  const typeMatch = typeRE.exec(injectFileContent);
                  if (typeMatch) {
                    let typeSubContent = injectFileContent.slice(
                      typeMatch.index,
                    );
                    const spaces = typeMatch[1] ? typeMatch[1] : "";
                    const endStr = `^${spaces}\\};`;
                    const endRE = new RegExp(endStr, "m");
                    const endMatch = endRE.exec(typeSubContent);
                    if (endMatch) {
                      typeSubContent = typeSubContent.slice(
                        0,
                        endMatch.index + endMatch[0].length,
                      );
                    } else {
                      typeSubContent = "Error capturing type declaration.";
                    }
                    typeContent.push(utils.removeLeadingSpaces(typeSubContent));
                  }
                }
                injectFileContent = typeContent.join("\n").trim();
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
                        matches[match].indexOf("-pause") + 7,
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
                options,
              );
              res = res.replace(replacer, injectFileContent);
              res = await addMarkdownIncludes(res);
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
                options,
              );
              // Temporarily replace double spaces with tabs. Replaced back downstream.
              // Prevents unexpected whitespace removal from util functions.
              res = res.replace(
                replacer,
                processedFileContent.replace(/ {2}/g, `\t`),
              );
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
      }
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

  fileString = replacePlaceHolders(await addMarkdownIncludes(fileString));

  return callback && callback(null, fileString);
};

module.exports = addCodeInject;
