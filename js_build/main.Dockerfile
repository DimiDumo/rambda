# Use the latest LTS version of Node.js with Alpine
FROM node:20-alpine

# Install pnpm
RUN npm install -g pnpm

# Install git to clone the repository
RUN apk add --no-cache git


# Set environment variables
ARG GITHUB_ACCESS_TOKEN
ARG REPO_NAME
ARG BRANCH_NAME

# Clone the private GitHub repository and force rebuild from here onward
ARG CACHE_BUST=1
RUN git clone https://$GITHUB_ACCESS_TOKEN@github.com/dimidumo/$REPO_NAME.git /app

# Set the working directory
WORKDIR /app

RUN git checkout $BRANCH_NAME

# Install dependencies using pnpm
RUN pnpm install

# Copy a local JavaScript file into the container
COPY main-exex-manager.js /app

# Manually install a specific dependency
# RUN pnpm add amqplib

# Command to run your application (adjust as necessary)
CMD ["node", "main-exex-manager.js"]
