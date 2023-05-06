import os


def pytest_configure(config):
    os.environ["GIT_AUTHOR_NAME"] = "Example user"
    os.environ["GIT_AUTHOR_EMAIL"] = "user@example.com"
    os.environ["GIT_COMMITTER_NAME"] = "Example user"
    os.environ["GIT_COMMITTER_EMAIL"] = "user@example.com"


def pytest_unconfigure(config):
    pass
