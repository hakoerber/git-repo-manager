#!/usr/bin/env python3

from helpers import *


def test_invalid_command():
    cmd = grm(["whatever"], is_invalid=True)
    assert "USAGE" in cmd.stderr


def test_help():
    cmd = grm(["--help"])
    assert "USAGE" in cmd.stdout
