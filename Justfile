lint:
    cargo clippy --no-deps

lint-fix:
    cargo clippy --no-deps --fix

release:
    cargo build --release
