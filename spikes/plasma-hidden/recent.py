"""Compares the user's two lists of recent files without printing any of them: the newest file:// bookmarks of
recently-used.xbel and the newest file resources in a copy of kactivitymanagerd's database. Prints only counts and
file extensions, since the paths are the user's.

usage: uv run --no-project python -I -B recent.py <recently-used.xbel> <copy of the database> [count]
"""

import sqlite3
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import PurePosixPath
from urllib.parse import unquote, urlparse

EXIT_USAGE = 64
DEFAULT_COUNT = 10
COMPARED = 5
FILE_SCHEME = "file"


def xbel_newest(path: str, count: int) -> list[str]:
    bookmarks = []
    for bookmark in ElementTree.parse(path).getroot().iter("bookmark"):
        href = bookmark.get("href", "")
        if urlparse(href).scheme != FILE_SCHEME:
            continue
        # ISO 8601 stamps in one zone sort as strings.
        stamp = bookmark.get("modified") or bookmark.get("visited") or bookmark.get("added") or ""
        bookmarks.append((stamp, unquote(urlparse(href).path)))
    bookmarks.sort(reverse=True)
    return [path for _, path in bookmarks[:count]]


def database_newest(path: str, count: int) -> list[str]:
    # Opened read-only even though it is a copy, so nothing rewrites its journal either.
    connection = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        rows = connection.execute(
            "SELECT targettedResource, MAX(lastUpdate) AS newest FROM ResourceScoreCache"
            " WHERE targettedResource LIKE '/%' OR targettedResource LIKE 'file://%'"
            " GROUP BY targettedResource ORDER BY newest DESC LIMIT ?",
            (count,),
        ).fetchall()
    finally:
        connection.close()
    return [unquote(urlparse(resource).path) if resource.startswith("file://") else resource for resource, _ in rows]


def extensions(paths: list[str]) -> str:
    return " ".join(PurePosixPath(path).suffix.lower() or "(none)" for path in paths)


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3):
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return EXIT_USAGE
    count = int(argv[2]) if len(argv) == 3 else DEFAULT_COUNT
    xbel = xbel_newest(argv[0], count)
    database = database_newest(argv[1], count)
    print(f"xbel: {len(xbel)} newest file:// bookmarks, extensions {extensions(xbel)}")
    print(f"kactivitymanagerd: {len(database)} newest file resources, extensions {extensions(database)}")
    first_xbel, first_database = xbel[:COMPARED], database[:COMPARED]
    shared = len(set(first_xbel) & set(first_database))
    same_place = sum(1 for a, b in zip(first_xbel, first_database) if a == b)
    print(f"first {COMPARED}: {shared} in both lists, {same_place} at the same place")
    in_either = len(set(xbel) & set(database))
    print(f"all {count}: {in_either} in both lists")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
