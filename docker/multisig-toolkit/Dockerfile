FROM node:23-bookworm-slim@sha256:dcacc1ee3b03a497c2096b0084d3a67b856e777b55ffccfcc76bcdab9cc65906 AS builder

# Set working directory
WORKDIR /app

# Install pnpm globally
RUN npm install -g pnpm

# Copy application source code with proper ownership
COPY --chown=node:node ./dapps/multisig-toolkit .

# Install dependencies using pnpm
RUN pnpm install

# Build the application
RUN pnpm build

# Use the same Node.js image for production, pinned to the same hash
FROM node:23-bookworm-slim@sha256:dcacc1ee3b03a497c2096b0084d3a67b856e777b55ffccfcc76bcdab9cc65906

# Install dumb-init for proper process management with SHA256 verification
RUN apt -qq update && apt -qq install -y wget && \
    wget -O /usr/local/bin/dumb-init https://github.com/Yelp/dumb-init/releases/download/v1.2.5/dumb-init_1.2.5_x86_64 && \
    echo "e874b55f3279ca41415d290c512a7ba9d08f98041b28ae7c2acb19a545f1c4df /usr/local/bin/dumb-init" | sha256sum -c - && \
    chmod +x /usr/local/bin/dumb-init && \
    apt -qq clean && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Install a simple static file server
RUN npm install -g serve

# Copy the built app from the builder stage with proper ownership
COPY --chown=node:node --from=builder /app/dist /app

# Set environment variables
ENV NODE_ENV production
ENV PORT 4173

# Expose port from environment variable
EXPOSE ${PORT}

# Switch to non-root user
USER node

# Serve the static files using dumb-init as the init process
CMD ["dumb-init", "serve", "-s", ".", "-l", "4173"]
