# Build the project in release mode
build:
    cargo build --release

# Build the project in debug mode
build-debug:
    cargo build

# Run the project
run:
    cargo run

# Run clippy with strict warnings
lint:
    cargo clippy --all-targets -- -D warnings

# Format code with rustfmt
fmt:
    cargo fmt --all

# Check code formatting
format:
    cargo fmt --all --check

# Run all tests
test:
    cargo test --all

# Check code without building
check:
    cargo check --all-targets

# Generate documentation
doc:
    cargo doc --no-deps

# Install the binary to ~/.cargo/bin
install:
    cargo install --path . --force
