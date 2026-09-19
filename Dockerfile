# ==========================================
# Stage 1: Build the Rust application
# ==========================================
# Edition 2024 requires Rust 1.85 or newer
FROM rust:latest AS builder

# Install build dependencies (needed for some crates that compile C bindings or OpenSSL)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# 1. Copy only the manifests first
COPY Cargo.toml Cargo.lock* ./

# 2. Create dummy source files to trick Cargo into downloading dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "" > src/lib.rs

# 3. Build the dependencies (This layer is cached unless Cargo.toml changes)
RUN cargo build --release

# 4. Remove dummy files and copy the actual source code
RUN rm -rf src
COPY src ./src

# 5. Touch the source files to ensure Cargo rebuilds your actual code
RUN touch src/main.rs src/lib.rs

# 6. Build the final application binary
RUN cargo build --release


# ==========================================
# Stage 2: Create the minimal runtime image
# ==========================================
FROM debian:bookworm-slim

# Install runtime dependencies (ca-certificates is REQUIRED for MongoDB TLS/SSL connections)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/topo /app/topo

# Expose the default Actix-web port
EXPOSE 8080

# Set default environment variables
ENV RUST_LOG=info

# Run the binary
CMD ["/app/topo"]