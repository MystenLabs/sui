import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

async function processDir(dirname) {
  const files = await fs.promises.readdir(dirname, {
    withFileTypes: true,
  });

  for (const file of files) {
    if (file.name.includes(" ")) {
      await fs.promises.rename(
        path.join(dirname, file.name),
        path.join(dirname, file.name.trim())
      );
    }

    if (file.isDirectory()) {
      await processDir(path.join(dirname, file.name.trim()));
    }
  }
}

async function main() {
  const dirname = fileURLToPath(new URL(".", import.meta.url));
  await processDir(path.join(dirname, "../svgs"));
}

main().catch(console.error);
