"""Parse pinned upstream display metadata as data, without running JavaScript."""

import os
from pathlib import Path
import re
import tempfile

REPOSITORY = "https://github.com/3641397194-wq/gpt6-Astra"
REVISION = "a30126e7c7d6c2ffe6b08f007cbc055d9b498f71"
METADATA_FILE = "desktop/src/shared/workbench-core.js"


def seat_labels(inputs):
    text = inputs[METADATA_FILE].decode("utf-8")
    seats = re.search(r"const SEATS = \[(.*?)\];", text, re.DOTALL)
    if not seats:
        raise ValueError("Missing upstream seat metadata")
    deepseek = re.findall(r"const DEEPSEEK_LABEL = '([^'\r\n]+)'", text)
    labels = {}
    for seat, literal, constant in re.findall(
        r"\{id:'([^']+)',tag:(?:'([^'\r\n]*)'|(DEEPSEEK_LABEL))", seats[1]
    ):
        label = literal if not constant else (deepseek[0] if len(deepseek) == 1 else "")
        if seat in labels or not label.strip():
            raise ValueError(f"Invalid or duplicate upstream seat label: {seat}")
        labels[seat] = label
    return labels


def codex_label(inputs):
    label = seat_labels(inputs).get("codex")
    if not label:
        raise ValueError("Missing upstream Codex seat label")
    return label


def refresh_codex_header(text, inputs):
    # The raw upstream Codex header still names GPT-6 while the workbench and
    # prompt-engine select GPT-6.2. Align only this display line, not task rules.
    text, count = re.subn(r"^# seat: [^\r\n]+ / Codex CLI$",
                         lambda _: f"# seat: {codex_label(inputs)} / Codex CLI",
                         text, flags=re.MULTILINE)
    if count != 1:
        raise ValueError("Expected one upstream Codex seat header")
    return text


def atomic_write(path, data):
    if path.exists() and path.read_bytes() == data:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(data)
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
