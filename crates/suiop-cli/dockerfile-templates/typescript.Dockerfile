# this dockerfile was created based on a template that implements the
# security practices outlined in 
# https://cheatsheetseries.owasp.org/cheatsheets/NodeJS_Docker_Cheat_Sheet.html#nodejs-docker-cheat-sheet

# we use node as the base image, pinned to a specific hash for reproducibility
# and to avoid sbom vulnerabilities
# --> build image
FROM node:18.17.1-bookworm-slim@sha256:e5c8c319295f6cbc288e19506a9ac37afa3b330f4e38afb01d1269b579cf6a5b AS builder
# this is the debian:12-slim build for node.

WORKDIR /app

RUN apt -qq update && apt -qq upgrade -y && apt -qq install -y npm && npm install -g typescript

COPY --chown=node:node . /app

COPY --chown=node:node src/ ./src
COPY --chown=node:node package.json package-lock.json tsconfig.json ./
COPY --chown=node:node src/ ./src/

RUN npm ci --omit=dev
RUN npm run build

# same hash as above, but using a separate container to avoid copying our 
# full build context
# --> runtime image
FROM node:18.17.1-bookworm-slim@sha256:e5c8c319295f6cbc288e19506a9ac37afa3b330f4e38afb01d1269b579cf6a5b 
RUN apt -qq update && apt install -y wget
# dumb-init serves as the init process, since node is not intended as one
# https://engineeringblog.yelp.com/2016/01/dumb-init-an-init-for-docker.html
RUN wget -O /usr/local/bin/dumb-init https://github.com/Yelp/dumb-init/releases/download/v1.2.5/dumb-init_1.2.5_x86_64
RUN chmod +x /usr/local/bin/dumb-init
WORKDIR /app

COPY --chown=node:node --from=builder /app/dist ./js/
COPY --chown=node:node --from=builder /app/node_modules/ ./node_modules/

# should be set to ensure performance and security optimizations are applied
ENV NODE_ENV production
ENV PORT 8080

EXPOSE ${PORT}

USER node
CMD ["dumb-init", "node", "js/index.js"]