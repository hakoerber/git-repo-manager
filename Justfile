set positional-arguments

target := "x86_64-unknown-linux-musl"

check: test
    cargo check
    cargo fmt --check
    cargo clippy --no-deps -- -Dwarnings

fmt:
    cargo fmt
    git ls-files | grep '\.py$' | xargs black

lint:
    cargo clippy --no-deps

lint-fix:
    cargo clippy --no-deps --fix

release:
    cargo build --release --target {{target}}

test-binary:
    env \
        GITHUB_API_BASEURL=http://rest:5000/github \
        GITLAB_API_BASEURL=http://rest:5000/gitlab \
        cargo build --target {{target}} --profile e2e-tests --features=static-build

install:
    cargo install --path .

install-static:
    cargo install --target {{target}} --features=static-build --path .

build:
    cargo build

build-static:
    cargo build --target {{target}} --features=static-build

test: test-unit test-integration test-e2e

test-unit +tests="":
    cargo test --lib --bins -- --show-output {{tests}}

test-integration:
    cargo test --test "*"

test-e2e +tests=".": test-binary
    cd ./e2e_tests \
    && docker-compose rm --stop -f \
    && docker-compose build \
    && docker-compose run \
        --rm \
        -v $PWD/../target/{{target}}/e2e-tests/grm:/grm \
            pytest \
            "GRM_BINARY=/grm ALTERNATE_DOMAIN=alternate-rest python3 -m pytest -p no:cacheprovider --color=yes "$@"" \
    && docker-compose rm --stop -f

update-dependencies: update-cargo-dependencies

update-cargo-dependencies:
    @cd ./depcheck \
    && python3 -m venv ./venv \
    && . ./venv/bin/activate \
    && pip --disable-pip-version-check install -r ./requirements.txt > /dev/null \
    && ./update-cargo-dependencies.py
