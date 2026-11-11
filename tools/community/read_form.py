#!/usr/bin/env python3
"""Split a "Compatibility recipe" issue body into recipe.toml and reports.json.

Usage: BODY=<issue body> read_form.py <out dir>. The body is untrusted: it
is only written to files, never interpreted.
"""
import os
import re
import sys


def section(body: str, title: str) -> str:
    m = re.search(r"^### " + re.escape(title) + r"\s*$(.*?)(?=^### |\Z)", body, re.S | re.M)
    if not m:
        return ""
    text = m.group(1).strip()
    fence = re.search(r"```[a-z]*\n(.*?)\n```", text, re.S)
    return (fence.group(1) if fence else text).strip() + "\n"


def main() -> None:
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    body = os.environ.get("BODY", "")
    with open(os.path.join(out, "recipe.toml"), "w") as f:
        f.write(section(body, "Recipe"))
    with open(os.path.join(out, "reports.json"), "w") as f:
        f.write(section(body, "Session reports"))


if __name__ == "__main__":
    main()
