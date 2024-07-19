# Use the latest LTS version of Node.js with Alpine
FROM node:20-alpine

# Install pnpm
RUN npm install -g pnpm

# Set environment variables
ENV GITHUB_TOKEN=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

# Install git to clone the repository
RUN apk add --no-cache git

# Clone the private GitHub repository
RUN git clone https://${GITHUB_TOKEN}@github.com/dimidumo/rambda_js_exex.git /app

# Set the working directory
WORKDIR /app

# Install dependencies using pnpm
RUN pnpm install

# Copy a local JavaScript file into the container
COPY exex-manager.js /app

# Manually install a specific dependency
# TODO: fix a version for redis
RUN pnpm add amqplib

# Command to run your application (adjust as necessary)
CMD ["node", "exex-manager.js"]
