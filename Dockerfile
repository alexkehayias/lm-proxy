# Build stage
FROM rust:bookworm AS builder

WORKDIR /

## Cache rust dependencies
## https://stackoverflow.com/questions/58473606/cache-rust-dependencies-with-docker-build
RUN mkdir ./src && echo 'fn main() { println!("Dummy!"); }' > ./src/main.rs
COPY ./Cargo.toml .
RUN cargo build --release

## Actually build the app
RUN rm -rf ./src
COPY ./src ./src
RUN touch -a -m ./src/main.rs
RUN cargo build --release

# Run stage
FROM debian:bookworm-slim AS runner

RUN apt-get update && \
    apt-get install -y --no-install-recommends libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

ARG HOST=0.0.0.0
ARG PORT=4000
ARG UPSTREAM
ARG METRICS_URL
ENV HOST=${HOST} PORT=${PORT} UPSTREAM=${UPSTREAM} METRICS_URL=${METRICS_URL}
COPY entrypoint.sh /entrypoint.sh
RUN ["chmod", "+x", "/entrypoint.sh"]

ARG USER=appuser
RUN addgroup --system $USER && adduser --system --ingroup $USER $USER

# Use the compiled binary rather than cargo
COPY --from=builder /target/release/lm-proxy /lm-proxy

USER $USER

ENV RUST_LOG=info

EXPOSE 4000

ENTRYPOINT ["./entrypoint.sh"]