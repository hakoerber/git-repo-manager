set positional-arguments

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

test-binary-docker:
    env \
        GITHUB_API_BASEURL=http://rest:5000/github \
        GITLAB_API_BASEURL=http://rest:5000/gitlab \
        cargo build --profile e2e-tests

test-binary:
    env \
        GITHUB_API_BASEURL=http://localhost:5000/github \
        GITLAB_API_BASEURL=http://localhost:5000/gitlab \
        cargo build --profile e2e-tests

install:
    cargo install --path .

test: test-unit test-integration test-e2e

test-unit:
    cargo test --lib --bins

test-integration:
    cargo test --test "*"

test-e2e-docker +tests=".": test-binary-docker
    cd ./e2e_tests \
    && docker-compose rm --stop -f \
    && docker-compose build \
    && docker-compose run \
        --rm \
        -v $PWD/../target/e2e-tests/grm:/grm \
            pytest \
            "GRM_BINARY=/grm python3 ALTERNATE_DOMAIN=alternate-rest -m pytest -p no:cacheprovider --color=yes "$@"" \
    && docker-compose rm --stop -f

test-e2e +tests=".": test-binary
    cd ./e2e_tests \
    && docker-compose rm --stop -f \
    && docker-compose build \
    && docker-compose up -d rest \
    && GRM_BINARY={{justfile_directory()}}/target/e2e-tests/grm ALTERNATE_DOMAIN=127.0.0.1 python3 -m pytest -p no:cacheprovider --color=yes {{tests}} \
    && docker-compose rm --stop -f

update-dependencies: update-cargo-dependencies

update-cargo-dependencies:
    @cd ./depcheck \
    && python3 -m venv ./venv \
    && . ./venv/bin/activate \
    && pip --disable-pip-version-check install -r ./requirements.txt > /dev/null \
    && ./update-cargo-dependencies.py
