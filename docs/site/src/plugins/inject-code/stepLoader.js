// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const rewrite = async (content) => {
  let stepCounter = 0;
  let substepCounter = 0;
  const lines = content.split("\n");
  return lines
    .map((line) => {
      const stepMatch = line.match(/^(#+)step\s+(.*)$/i);
      const substepMatch = line.match(/^(#+)substep\s+(.*)$/i);

      if (stepMatch) {
        const [_, hashes, title] = stepMatch;
        stepCounter += 1;
        substepCounter = 0;
        return `${hashes} Step ${stepCounter}: ${title}`;
      }

      if (substepMatch) {
        const [_, hashes, title] = substepMatch;
        substepCounter += 1;
        return `${hashes} Step ${stepCounter}.${substepCounter}: ${title}`;
      }

      return line;
    })
    .join("\n");
};

module.exports = function (source) {
  const callback = this.async(); // mark the loader as async

  rewrite(source)
    .then((result) => callback(null, result))
    .catch((err) => callback(err));
};
