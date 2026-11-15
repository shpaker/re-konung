"""Build a local macOS application bundle for interactive development and testing."""

import argparse
import json
import plistlib
import shutil
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game-dir", type=Path, required=True)
    arguments = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("application bundles require macOS; use just game on other platforms")
    game_dir = arguments.game_dir.resolve(strict=True)
    if not game_dir.is_dir():
        parser.error("--game-dir must be a directory")

    root = Path(__file__).resolve().parents[1]
    manifest = root / "crates/Cargo.toml"
    subprocess.run(
        [
            "cargo",
            "build",
            "--manifest-path",
            str(manifest),
            "--profile",
            "dev-runtime",
            "-p",
            "kn-player",
            "--features",
            "kn-fe-player/dynamic",
        ],
        cwd=root,
        check=True,
    )
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--manifest-path",
                str(manifest),
                "--no-deps",
                "--format-version",
                "1",
            ],
            cwd=root,
            text=True,
        )
    )
    target = Path(metadata["target_directory"]) / "dev-runtime"
    rust_libraries = subprocess.check_output(
        ["rustc", "--print", "target-libdir"],
        cwd=root,
        text=True,
    ).strip()
    output = root / "out/macos"
    bundle = output / "re-konung.app"
    executables = bundle / "Contents/MacOS"
    executables.mkdir(parents=True, exist_ok=True)
    executable = executables / "re-konung"
    replacement = executable.with_suffix(".next")
    shutil.copy2(target / "re-konung", replacement)
    replacement.replace(executable)

    with (bundle / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump(
            {
                "CFBundleExecutable": "re-konung",
                "CFBundleIdentifier": "io.github.shpaker.re-konung.dev",
                "CFBundleName": "re-konung",
                "CFBundleDisplayName": "re-konung",
                "CFBundlePackageType": "APPL",
                "CFBundleVersion": "1",
                "CFBundleShortVersionString": "0.1.0",
                "NSHighResolutionCapable": True,
                "LSEnvironment": {
                    "RE_KONUNG_GAME_DIR": str(game_dir),
                    "RE_KONUNG_SETTINGS": str(output / "settings.json"),
                    "DYLD_LIBRARY_PATH": rust_libraries + ":" + str(target / "deps"),
                },
            },
            stream,
        )
    print(f"Open {bundle}")
    print(f"Development settings: {output / 'settings.json'}")


if __name__ == "__main__":
    main()
