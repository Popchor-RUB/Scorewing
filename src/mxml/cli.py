"""Command-line interface for the two stave transformations."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from lxml import etree as ET

from . import __version__
from .io import check_output, read_score, write_score
from .model import ScoreError


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(prog="mxml", description="Split and merge choir staves in MusicXML scores.")
    result.add_argument("--version", action="version", version=f"mxml {__version__}")
    modes = result.add_subparsers(dest="command", required=True)
    for name, help_text in (("split", "Retain original staves and add separate voices below each"),
                            ("merge", "Merge explicitly selected staves into numbered voices")):
        command = modes.add_parser(name, help=help_text)
        command.add_argument("input", type=Path, help="Plain MusicXML or compressed .mxl input")
        command.add_argument("-o", "--output", type=Path, required=True, help="Output file; extension selects format")
        command.add_argument("--force", action="store_true", help="Replace an existing output file")
        if name == "split":
            command.add_argument("--mode", choices=("chords", "voices"), default="chords",
                                 help="Separate chord tones (default) or retain chords within explicit voices")
        else:
            command.add_argument("--pair", action="append", required=True, metavar="STAVE+STAVE[+STAVE]",
                                 help="One-based positions or exact part names; repeat for multiple groups")
    return result


def main(argv: list[str] | None = None) -> int:
    arguments = parser().parse_args(argv)
    try:
        check_output(arguments.input, arguments.output, arguments.force)
        score = read_score(arguments.input)
        if arguments.command == "split":
            from .split import split_score

            root = split_score(score, arguments.mode)
        else:
            from .merge import merge_score

            root = merge_score(score, arguments.pair)
        write_score(root, arguments.output, input_path=arguments.input, force=arguments.force)
    except (ScoreError, OSError, ET.Error, ValueError, OverflowError) as error:
        print(f"mxml: {error}", file=sys.stderr)
        return 2
    return 0
