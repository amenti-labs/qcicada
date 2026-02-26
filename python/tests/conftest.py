"""Shared fixtures and markers for qcicada tests."""

import os
import pytest
from qcicada import QCicada, find_devices


def pytest_configure(config):
    config.addinivalue_line(
        "markers", "device: requires a physical QCicada device"
    )


def pytest_collection_modifyitems(config, items):
    """Skip device tests if no QCicada is connected."""
    port = os.environ.get("QCICADA_PORT")
    if port:
        return  # user explicitly set a port, run all tests

    if find_devices():
        return  # device detected, run all tests

    skip = pytest.mark.skip(reason="No QCicada device connected")
    for item in items:
        if "device" in item.keywords:
            item.add_marker(skip)


@pytest.fixture
def qrng():
    """Open a QCicada device for the test, close it after."""
    port = os.environ.get("QCICADA_PORT")
    dev = QCicada(port=port)
    yield dev
    dev.close()
