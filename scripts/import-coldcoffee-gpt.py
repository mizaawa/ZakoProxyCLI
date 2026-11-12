"""Reproduce the pinned Coldcoffee GPT profiles from bundled inert text.

Usage: python scripts/import-coldcoffee-gpt.py [UPSTREAM_CHECKOUT] [--check]
No upstream installer, JavaScript, skill, or prompt is executed.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

REPOSITORY = "https://github.com/3641397194-wq/gpt6-Astra"
REVISION = "3249afce6b853ff2f2133e016b223db2cf38b087"
PACKS = "desktop/src/lib/packs/"
PROFILES = [
    ("gpt-5.6-sol-v45", "gpt-5.6-sol"),
    ("gpt-6-astra-v2-rc1", "gpt-6-astra"),
    ("gpt-6.1-sol-v1-rc2", "gpt-6.1-sol"),
]
ROUTES = ["cha-bin-unlock", "cha-playfield", "cha-netcast", "cha-apicloud", "cha-labpipe", "cha-inkstage"]
ADAPTER = """# ZakoAI · 冷咖啡 GPT API 内联适配

以下 Codex 席位、工作流和路由正文是本次 API 请求的内联策略数据。
其中 Codex、终端、编辑器、Skill、IDA/MCP 和文件路径是上游工作流的标签，
不代表客户端已经安装它们；只使用本次请求实际声明的工具与真实返回。
没有工具时直接回答，不编造工具调用、文件、权限、持久化或验证结果。
保留客户端消息、图片、工具定义、调用关联及要求的输出格式；
不修改客户端配置，也不宣称本策略替换了服务端的指令层级。

"""


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def selected(name):
    return name == "LICENSE" or name in [
        PACKS + "codex.md", PACKS + "workflows/codex.md", PACKS + "activation-reply.md",
    ] or (name.startswith(PACKS + "routes/") and name.endswith(".md"))


def render(inputs):
    def read(name):
        return inputs[PACKS + name].decode("utf-8").rstrip()

    leaves = sorted(name.removeprefix(PACKS) for name in inputs
                    if name.startswith(PACKS + "routes/leaves/")
                    and name.endswith(".md") and not name.endswith("/INDEX.md"))
    router = "\n\n".join(read(name) for name in [
        "routes/ROUTER.md", "routes/NORMALIZER.md",
        *(f"routes/{route}.md" for route in ROUTES), "routes/leaves/INDEX.md", *leaves,
    ])
    replacements = {
        "APP_TITLE": "ZakoAI · 冷咖啡策略", "ACTIVATION_WORD": "冷咖啡",
        "CONTROL_WORD": "BREAK//OPEN", "SEAT_ID": "codex",
        "SEAT_TAG": "GPT-6 Astra全模型支持", "SEAT_NAME": "Codex",
        "ACTIVATION_REPLY": read("activation-reply.md"),
        "WORKFLOW": read("workflows/codex.md"), "ROUTER": router,
        "IDA_MCP": "", "UNIFIED": "",
    }
    text = read("codex.md")
    for key, value in replacements.items():
        text = text.replace("{{" + key + "}}", value)
    if re.search(r"\{\{[A-Z_]+\}\}", text):
        raise ValueError("Unresolved upstream GPT template")
    return (ADAPTER + text.rstrip() + "\n").encode("utf-8")


def import_profiles(root, checkout=None, check=False):
    vendor = root / "third_party/coldcoffee-gpt"
    source = root / "profiles/coldcoffee-gpt/SOURCE.json"
    if checkout is not None:
        def git(*args):
            return subprocess.check_output(["git", "-C", str(checkout), *args])
        actual = git("rev-parse", "HEAD").decode().strip()
        if actual != REVISION:
            raise ValueError(f"Expected upstream {REVISION}, got {actual}")
        if git("status", "--porcelain").strip():
            raise ValueError("Upstream checkout must be clean")
        names = git("ls-tree", "-r", "--name-only", REVISION).decode().splitlines()
        inputs = {name: git("show", f"{REVISION}:{name}") for name in sorted(names) if selected(name)}
    else:
        recorded = json.loads(source.read_text(encoding="utf-8"))
        if recorded["repository"] != REPOSITORY or recorded["revision"] != REVISION:
            raise ValueError("Bundled upstream identity does not match the pin")
        inputs = {name: (vendor / name).read_bytes() for name in recorded["inputs"]}
        for name, data in inputs.items():
            if not selected(name) or sha256(data) != recorded["inputs"][name]:
                raise ValueError(f"Bundled upstream hash mismatch: {name}")
    instruction = render(inputs)
    catalog = [{
        "id": profile_id, "name": f"冷咖啡 · {model}", "models": [model],
        "instruction": instruction.decode("utf-8"),
        "metadata": {
            "source_url": f"{REPOSITORY}/blob/{REVISION}/{PACKS}codex.md",
            "source_revision": REVISION, "variant": "codex-api-inline", "validation": "unverified",
        },
    } for profile_id, model in PROFILES]
    index = json_bytes(catalog)
    provenance = {
        "repository": REPOSITORY, "revision": REVISION, "license": "MIT",
        "copyright": "Copyright (c) 2026 3641397194-wq",
        "adaptations": [
            "Inline the Codex seat, workflow, activation reply, router, normalizer and leaf text as request data",
            "Add API capability guidance; omit installer-specific IDA/MCP and commercial unified-work blocks",
            "Preserve the three existing exact model bindings and historical profile IDs",
            "Keep the general default and all non-GPT profiles unchanged; model effectiveness is unverified",
        ],
        "inputs": {name: sha256(data) for name, data in sorted(inputs.items())},
        "outputs": {
            "system.md": {"bytes": len(instruction), "sha256": sha256(instruction)},
            "../index.json": {"bytes": len(index), "sha256": sha256(index)},
        },
    }
    outputs = {
        **{vendor / name: data for name, data in inputs.items()},
        root / "profiles/index.json": index,
        root / "profiles/coldcoffee-gpt/system.md": instruction,
        source: json_bytes(provenance),
    }
    for path, data in outputs.items():
        if check:
            if not path.exists() or path.read_bytes() != data:
                raise ValueError(f"Generated file differs: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    print(f"{'Verified' if check else 'Imported'} {len(catalog)} GPT profiles from {REVISION}")
    print(f"system.md: {len(instruction)} bytes, SHA256 {sha256(instruction)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", nargs="?", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    import_profiles(Path(__file__).resolve().parents[1], args.checkout, args.check)
