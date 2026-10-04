# FROM node:23-bookworm-slim@sha256:dcacc1ee3b03a497c2096b0084d3a67b856e777b55ffccfcc76bcdab9cc65906 AS builder
FROM stagex/pallet-nodejs@sha256:07c6e3d54dfa7df17134ebab33310442adceeca1d980cbf7169a6f15737cd0e0 AS nodejs
FROM nodejs AS builder

# Set working directory
WORKDIR /app

# Install pnpm globally
# RUN npm install -g pnpm

# Copy application source code with proper ownership
# COPY --chown=node:node ./dapps/multisig-toolkit .
COPY ./dapps/multisig-toolkit .

# Install dependencies
RUN npm clean-install

# Build the application
RUN --network=none npm run build

# Use the same Node.js image for production, pinned to the same hash
FROM nodejs

# This is a bug. Currently, nodejs pallet sets the user as `user`, which
# doesn't have permission to install globally.
USER root

# Set working directory
WORKDIR /app

# Install a simple static file server
# RUN npm install -g --verbose serve

# Copy the built app from the builder stage with proper ownership
COPY --chown=user:user --from=builder /app/dist /app

# Set environment variables
ENV NODE_ENV=production
ENV PORT=4173

# Expose port from environment variable
EXPOSE ${PORT}

# Switch to non-root user to drop privileges
USER user

ENV NODE_PATH=/usr/lib/node_modules

# Serve the static files using dumb-init as the init process
CMD ["/usr/bin/serve", "-s", ".", "-l", "4173"]
