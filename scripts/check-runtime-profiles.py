"""Verify a fresh console's /api/profiles JSON against the checked-out sources."""

import json
from pathlib import Path
import sys


def expected_profiles(root):
    def text(path):
        return (root / path).read_text(encoding="utf-8")

    def metadata(path):
        return json.loads(text(path))

    catalog = metadata("src/relay-catalog.json")["models"]
    gpt = {p["id"]: p["instruction"] for p in metadata("third_party/gpt-instruct/profiles.legacy.json")}
    expected = {}
    for profile_id, models in catalog.items():
        if profile_id.startswith("gpt-"):
            body, source = gpt[profile_id], "third_party/gpt-instruct/SOURCE.json"
        elif profile_id.startswith("hanshuang-") or profile_id == "coldcoffee-deepseek-v4.1-flash-v1":
            body, source = text("profiles/hanshuang/system.md"), "profiles/hanshuang/SOURCE.json"
        elif profile_id.startswith("kimi-break-"):
            body, source = text("profiles/kimi-break/system.md"), "profiles/kimi-break/SOURCE.json"
        elif profile_id.startswith("aijailbreak-"):
            family = models[0].split("-", 1)[0]
            body, source = text(f"profiles/ai-jailbreak/{family}.md"), "profiles/ai-jailbreak/SOURCE.json"
        elif profile_id.startswith("coldcoffee-"):
            family = models[0].split("-", 1)[0]
            template = {"gpt": "codex", "glm": "glm53"}.get(family, family)
            body, source = text(f"profiles/coldcoffee-full/{template}.md"), "profiles/coldcoffee-full/SOURCE.json"
        else:
            raise ValueError(f"No source assigned to {profile_id}")
        if models[0].startswith("deepseek-"):
            body += "\n\n" + text("profiles/dsh-purge/api-adapter.md")
        expected[profile_id] = {
            "models": models, "instruction": body,
            "repository": metadata(source)["repository"],
        }
    return expected


def check(root, records):
    expected = expected_profiles(root)
    if not isinstance(records, list) or len(records) != len(expected):
        raise ValueError("Runtime profile count differs from source catalog")
    if {p["id"] for p in records} != set(expected):
        raise ValueError("Runtime profile IDs differ from source catalog")
    for record in records:
        profile_id = record["id"]
        source = expected[profile_id]
        for field in ["models", "instruction"]:
            if record[field] != source[field]:
                raise ValueError(f"Runtime {field} differs: {profile_id}")
        meta = record.get("metadata") or {}
        url = meta.get("source_url", "")
        if url != source["repository"] and not url.startswith(source["repository"] + "/"):
            raise ValueError(f"Runtime provenance differs: {profile_id}")
        if meta.get("validation") != "unverified":
            raise ValueError(f"Runtime model-effectiveness status differs: {profile_id}")
    return len(expected)


if __name__ == "__main__":
    try:
        count = check(Path(__file__).resolve().parents[1], json.load(sys.stdin.buffer))
        print(f"PASS runtime catalog: {count} exact source bodies and model bindings")
    except (OSError, ValueError, KeyError, TypeError) as exc:
        sys.exit(f"FAIL runtime catalog: {exc}")
