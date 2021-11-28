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

test: test-unit test-integration

test-unit:
    cargo test --lib --bins

test-integration:
    cargo test --test "*"

update-dependencies:
    @cd ./depcheck \
    && python3 -m venv ./venv \
    && . ./venv/bin/activate \
    && pip --disable-pip-version-check install -r ./requirements.txt > /dev/null \
    && ./update-cargo-dependencies.py
