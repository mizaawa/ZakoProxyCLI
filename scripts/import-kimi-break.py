"""Import inert Kimi prompt data from a pinned checkout or bundled source.

Usage: python scripts/import-kimi-break.py [UPSTREAM_CHECKOUT] [--check]
Without a checkout, regeneration is fully offline. Upstream text is never run.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

REVISION = "c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6"
REPOSITORY = "https://github.com/YD-233/kimi-break"
MODELS = ["kimi-k3", "kimi-k2.7", "kimi-k2.7-code", "kimi-k2.6"]
FILES = ["README.md", "SYSTEM.md", "LICENSE"]
ADAPTER = """# ZakoAI · kimi-break API 内联适配

以下正文由 API 中转层随本次请求提供。运行环境以本次请求的消息、工具定义和
真实工具返回为准，不预设 Kimi Code CLI、终端、文件系统、工作目录、AGENTS.md、
Skill 或插件已经存在。不安装或加载本地文件，不伪造工具调用或执行结果。
保留客户端消息、图片、工具参数及调用关联；只调用实际提供且可用的工具。

"""
ENVIRONMENT = """# Environment

This request is supplied through an API client. No operating system, shell,
working directory, directory listing, local permissions, or current date is
assumed. Use only environment details explicitly supplied by the client or real
tool results. Only tools declared in this request are available; a prompt naming
a tool does not install it or prove that a call ran.

# Project information

Use project guidance, files, and skills only when supplied in the conversation or
returned by an available tool. This profile does not provide a local AGENTS.md,
skills directory, plugin runtime, or access to the client's computer.

"""


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def adapt(source):
    text = source.decode("utf-8")
    replacements = {
        "also an interactive general AI agent running on WREN's computer.":
            "also an interactive general AI agent serving WREN through an API client.",
        "${role_additional}":
            "Use the actual request and its tool definitions as the runtime context.",
        "${reply_style_guide}":
            "Follow the language and output format requested in the conversation.",
        "${notify_user_guidance}": "",
        "The dedicated tools resolve paths through the workspace access policy and cap their output, keeping large raw dumps out of the conversation.":
            "Use only tools actually declared by the API client; their path, permission, and output behavior is defined by that client.",
        "Tool calls run behind the WREN's permission settings.":
            "Respect any permissions and tool-call decisions supplied by the API client.",
    }
    for old, new in replacements.items():
        if text.count(old) != 1:
            raise ValueError(f"Expected one upstream adapter anchor: {old}")
        text = text.replace(old, new, 1)
    start = text.index("# Environment\n")
    end = text.index("---\n\n## user_style", start)
    text = text[:start] + ENVIRONMENT + text[end:]
    if re.search(r"\$\{[^}]+\}", text):
        raise ValueError("Unresolved upstream template variable")
    return (ADAPTER + text.rstrip() + "\n").encode("utf-8")


def import_profiles(root, checkout=None, check=False):
    vendor = root / "third_party/kimi-break"
    if checkout is not None:
        actual = subprocess.check_output(
            ["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True
        ).strip()
        if actual != REVISION:
            raise ValueError(f"Expected upstream {REVISION}, got {actual}")
        if subprocess.check_output(
            ["git", "-C", str(checkout), "status", "--porcelain"], text=True
        ).strip():
            raise ValueError("Upstream checkout must be clean")
        inputs = {name: subprocess.check_output([
            "git", "-C", str(checkout), "show", f"{REVISION}:{name}"
        ]) for name in FILES}
    else:
        inputs = {name: (vendor / name).read_bytes() for name in FILES}
        recorded = json.loads((root / "profiles/kimi-break/SOURCE.json").read_text(encoding="utf-8"))
        if recorded["repository"] != REPOSITORY or recorded["revision"] != REVISION:
            raise ValueError("Bundled upstream identity does not match the pin")
        for name, data in inputs.items():
            if sha256(data) != recorded["inputs"][name]:
                raise ValueError(f"Bundled upstream hash mismatch: {name}")
    instruction = adapt(inputs["SYSTEM.md"])
    catalog = [
        {"id": f"kimi-break-{model}-v1", "name": f"kimi-break · {model}",
         "models": [model], "model_family": "kimi"}
        for model in MODELS
    ]
    def json_bytes(value):
        return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    index = json_bytes(catalog)
    provenance = {
        "repository": REPOSITORY, "revision": REVISION,
        "license": "MIT", "copyright": "Copyright (c) 2026 YD-233",
        "adaptations": [
            "Keep upstream persona, behavioral strategy, coding guidance, and user style as inert prompt data",
            "Replace CLI-local environment/project sections and template variables with actual API request context",
            "Qualify computer, dedicated-tool, and permission assumptions using only client-supplied capabilities",
            "Register four exact model IDs, independently selectable through existing protocol injection",
        ],
        "inputs": {name: sha256(data) for name, data in inputs.items()},
        "outputs": {
            "system.md": {"bytes": len(instruction), "sha256": sha256(instruction)},
            "index.json": {"bytes": len(index), "sha256": sha256(index)},
        },
    }
    outputs = {
        **{vendor / name: data for name, data in inputs.items()},
        root / "profiles/kimi-break/system.md": instruction,
        root / "profiles/kimi-break/index.json": index,
        root / "profiles/kimi-break/SOURCE.json": json_bytes(provenance),
    }
    for path, data in outputs.items():
        if check:
            if not path.exists() or path.read_bytes() != data:
                raise ValueError(f"Generated file differs: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    print(f"{'Verified' if check else 'Imported'} {len(catalog)} exact-model profiles from {REVISION}")
    print(f"system.md: {len(instruction)} bytes, SHA256 {sha256(instruction)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", nargs="?", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    import_profiles(Path(__file__).resolve().parents[1], args.checkout, args.check)
