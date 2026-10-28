from importlib.metadata import version


def test_package_has_version():
    assert version("konung")
