# syntax=docker/dockerfile:1

FROM node:24-alpine AS dev
ARG UID=1000
ARG GID=1000
RUN corepack enable
RUN deluser node 2>/dev/null || true \
    && delgroup node 2>/dev/null || true \
    && (addgroup -g ${GID} node 2>/dev/null || true) \
    && (adduser -D -u ${UID} -G node node 2>/dev/null || true) \
    && mkdir -p /app/node_modules \
    && chown -R ${UID}:${GID} /app
WORKDIR /app
USER ${UID}:${GID}

FROM node:24-alpine AS build
WORKDIR /app
COPY package.json pnpm-lock.yaml ./
RUN corepack enable && pnpm install --frozen-lockfile --ignore-scripts
COPY . .
RUN pnpm build

FROM nginxinc/nginx-unprivileged:1.27-alpine AS runtime
COPY --from=build /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 8080