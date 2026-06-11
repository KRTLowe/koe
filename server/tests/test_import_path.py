from pathlib import Path

import kaya_server


def test_kaya_server_imports_from_repo_src():
    package_path = Path(kaya_server.__file__).resolve()
    repo_server_root = Path(__file__).resolve().parents[1]

    assert repo_server_root in package_path.parents
    assert package_path.parts[-3:-1] == ("src", "kaya_server")
