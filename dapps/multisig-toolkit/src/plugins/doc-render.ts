import path from "path";
import fs from "fs";

export default function docRender() {
  const rm = fs.readFileSync("./README.md", "utf-8")

  return ({
      content: rm
    })
}