FROM rust:1.77.0-slim-bookworm

COPY . .

RUN cargo build --release 

CMD ["./target/release/test-service"]
