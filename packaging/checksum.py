"""Write portable SHA-256 checksums for the built executable."""

import hashlib
from pathlib import Path
import sys


def main():
    for argument in sys.argv[1:]:
        path = Path(argument)
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        output = path.with_name(path.name + ".sha256")
        output.write_text(f"{digest}  {path.name}\n", encoding="ascii")
        print(output)


if __name__ == "__main__":
    main()
