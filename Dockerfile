# ---- build stage ----
FROM rust:1.90 AS builder
WORKDIR /app
COPY Cargo.toml .
COPY src ./src
COPY index.html .
RUN cargo build --release

# ---- runtime stage ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/vetclinic /usr/local/bin/vetclinic
EXPOSE 3000
CMD ["vetclinic"]
