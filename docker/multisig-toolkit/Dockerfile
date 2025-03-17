# Use a more standard Node.js image that includes a shell
FROM node:23-bookworm AS builder

# Set working directory
WORKDIR /app

# Install pnpm globally
RUN npm install -g pnpm

# Copy application source code
COPY ./dapps/multisig-toolkit .

# Install dependencies using pnpm
RUN pnpm install

# Build the application
RUN pnpm build

# Use a smaller Node.js image for production
FROM node:23-bookworm-slim

# Set working directory
WORKDIR /app

# Install a simple static file server
RUN npm install -g serve

# Copy the built app from the builder stage
COPY --from=builder /app/dist /app

# Expose port 4173
EXPOSE 4173

# Serve the static files
CMD ["serve", "-s", ".", "-l", "4173"]
