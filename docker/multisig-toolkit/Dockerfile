# Use a more standard Node.js image that includes a shell
FROM node:23-bookworm

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

# Expose port for the application
EXPOSE 4173

# Command to run the application with host flag to listen on all interfaces
CMD ["pnpm", "preview", "--host", "0.0.0.0"]
