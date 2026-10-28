"""Командная строка: `konung <команда>`."""

import argparse
import sys
from importlib.metadata import version
from pathlib import Path

from konung import disc


def cmd_disc(args: argparse.Namespace) -> None:
    for path in disc.extract(args.mds, args.out):
        print(path)


def main() -> None:
    # Консоль Windows по умолчанию не в UTF-8, а сообщения — на русском
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(prog="konung", description=__doc__)
    parser.add_argument("--version", action="version", version=version("konung"))
    commands = parser.add_subparsers(required=True)

    p = commands.add_parser("disc", help="образ MDS/MDF → track01.iso и trackNN.wav")
    p.add_argument("mds", type=Path)
    p.add_argument("out", type=Path)
    p.set_defaults(func=cmd_disc)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
