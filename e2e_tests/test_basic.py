#!/usr/bin/env python3

from helpers import grm


def test_invalid_command():
    cmd = grm(["whatever"], is_invalid=True)
    assert "usage" in cmd.stderr.lower()


def test_help():
    cmd = grm(["--help"])
    assert "usage" in cmd.stdout.lower()
