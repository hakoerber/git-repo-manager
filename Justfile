check:
    cargo check
    cargo fmt --check
    cargo clippy --no-deps

lint-fix:
    cargo clippy --no-deps --fix

release:
    cargo build --release

install:
    cargo install --path .

test:
    cargo test --lib --bins
