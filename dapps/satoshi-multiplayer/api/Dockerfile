FROM node:18-alpine

# Sets the default working directory to /app which is where we copy the service files to.
WORKDIR /app

# Denotes to copy all files in the service to 'app' folder in the container
COPY . /app

# Install service dependencies relevant for production builds skipping all development dependencies.
# RUN npm install --production --no-optional
RUN npm install

EXPOSE 8000/tcp

# Starts the service
CMD ["npm", "run", "dev"]