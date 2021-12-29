check: check-cargo-lock test
    cargo check
    cargo fmt --check
    cargo clippy --no-deps -- -Dwarnings

check-cargo-lock:
    cargo update --locked

lint-fix:
    cargo clippy --no-deps --fix

release:
    cargo build --release

install:
    cargo install --path .

test: test-unit test-integration test-e2e

test-unit:
    cargo test --lib --bins

test-integration:
    cargo test --test "*"

e2e-venv:
    cd ./e2e_tests \
    && python3 -m venv venv \
    && . ./venv/bin/activate \
    && pip --disable-pip-version-check install -r ./requirements.txt >/dev/null


test-e2e +tests=".": e2e-venv release
    cd ./e2e_tests \
    && . ./venv/bin/activate \
    && TMPDIR=/dev/shm python -m pytest --color=yes {{tests}}

update-dependencies:
    @cd ./depcheck \
    && python3 -m venv ./venv \
    && . ./venv/bin/activate \
    && pip --disable-pip-version-check install -r ./requirements.txt > /dev/null \
    && ./update-cargo-dependencies.py
