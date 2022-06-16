set positional-arguments

static_target := "x86_64-unknown-linux-musl"

check: fmt-check lint test
    cargo check

clean:
    cargo clean
    git clean -f -d -X

fmt:
    cargo fmt
    git ls-files | grep '\.py$' | xargs black
    git ls-files | grep '\.sh$' | xargs -L 1 shfmt --indent 4 --write

fmt-check:
    cargo fmt --check
    git ls-files | grep '\.py$' | xargs black --check
    git ls-files | grep '\.sh$' | xargs -L 1 shfmt --indent 4 --diff

lint:
    cargo clippy --no-deps -- -Dwarnings
    git ls-files | grep '\.sh$' | xargs -L 1 shellcheck --norc

lint-fix:
    cargo clippy --no-deps --fix

release:
    cargo build --release

release-static:
    cargo build --release --target {{static_target}} --features=static-build

test-binary:
    env \
        GITHUB_API_BASEURL=http://rest:5000/github \
        GITLAB_API_BASEURL=http://rest:5000/gitlab \
        cargo build --profile e2e-tests

install:
    cargo install --path .

install-static:
    cargo install --target {{static_target}} --features=static-build --path .

build:
    cargo build

build-static:
    cargo build --target {{static_target}} --features=static-build

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
        -v $PWD/../target/e2e-tests/grm:/grm \
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
