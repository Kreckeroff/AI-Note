FROM node:22-bookworm-slim AS frontend
WORKDIR /build/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY frontend/tsconfig.json frontend/vite.config.js ./
COPY frontend/src/ ./src/
RUN npm run build

FROM rust:1.98-bookworm AS backend
WORKDIR /build
COPY backend/Cargo.toml backend/Cargo.lock backend/build.rs ./backend/
COPY backend/migrations/ ./backend/migrations/
COPY backend/src/ ./backend/src/
COPY app/auth.css app/spaces.css app/sign-in.mjs app/space-form.mjs app/page-editor.js app/workspace-menu.mjs ./app/
COPY --from=frontend /build/frontend/dist/ ./frontend/dist/
RUN SEVER_REQUIRE_REACT=1 cargo build --release --locked --manifest-path backend/Cargo.toml

FROM debian:bookworm-slim
COPY --from=backend /build/backend/target/release/sever /usr/local/bin/sever
USER 10001:10001
CMD ["sever", "serve"]
