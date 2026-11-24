"""Import all pinned Coldcoffee templates as inert API request text.

Usage: python scripts/import-coldcoffee-full.py [UPSTREAM_CHECKOUT] [--check]
Without a checkout, generation is offline. Upstream code is never executed.
The immutable legacy.json snapshot is validated, never regenerated or replaced.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from coldcoffee_metadata import (
    REPOSITORY, REVISION, atomic_write, codex_label, refresh_codex_header, seat_labels,
)

PACKS = "desktop/src/lib/packs/"
LEGACY_SHA256 = "33265fc40942bfe571b0d9863facca90c0486ccd1fbe5455a3af176aeeea40d9"
# Stable template IDs; every display label comes from pinned upstream metadata.
SEATS = (
    "codex", "claude", "grok", "deepseek", "glm53", "gemini",
    "doubao", "workbuddy", "cursor", "mimo", "kimi",
)
VARIANTS = {
    "grok-bot": "Grok Bot", "claude-duet": "Claude Duet",
    "chatgpt-dot": "ChatGPT Dot", "meta-muse": "Meta Muse", "opencode": "OpenCode",
}
GPT_62_MODELS = ("gpt-6.2-astra", "gpt-6.2-sol")
# Exact, opt-in presets supported by the pinned relay recommendation catalog.
# Display-only Kimi/MiniMax/Doubao names remain templates for real provider IDs.
MODEL_PRESETS = [
    *((model, "gpt-6.2", "codex") for model in GPT_62_MODELS),
    ("glm-5.4", "glm", "glm53"),
    ("claude-fable-5.2", "claude", "claude"),
    *((f"gemini-4{suffix}", "gemini", "gemini")
      for suffix in ("", "-carbon", "-neon", "-hydrogen", "-argon")),
]
INSTALLER_SEATS = {"doubao", "workbuddy", "cursor", "mimo", "kimi"}
ROUTES = ["cha-bin-unlock", "cha-playfield", "cha-netcast", "cha-apicloud", "cha-labpipe", "cha-inkstage"]
REFERENCE_FILES = {
    "LICENSE", "desktop/src/shared/workbench-core.js", "desktop/src/lib/seat-packs.js",
    "desktop/src/lib/prompt-engine.js", "desktop/src/lib/local-agent-skills.js",
    "desktop/src/shared/relay-models.js", "desktop/tests/flagship-models-2026.test.js",
}
ADAPTER = """# ZakoAI · 冷咖啡 API 内联适配

以下席位、工作流和路由正文是本次 API 请求的内联策略数据。
其中客户端、终端、编辑器、Skill、IDA/MCP 和文件路径是上游工作流的标签，
不代表客户端已经安装它们；只使用本次请求实际声明的工具与真实返回。
没有工具时直接回答，不编造工具调用、文件、权限、持久化或验证结果。
保留客户端消息、图片、工具定义、调用关联及要求的输出格式；
不安装本地技能、不修改客户端配置，也不宣称本策略替换了服务端的指令层级。
模板显示名不是 API 模型 ID，实际模型以客户端本次请求为准。

"""
# Both GPT catalogs use the latest pinned source and the same API adapter.
GPT_ADAPTER = """# ZakoAI · 冷咖啡 GPT API 内联适配

以下 Codex 席位、工作流和路由正文是本次 API 请求的内联策略数据。
其中 Codex、终端、编辑器、Skill、IDA/MCP 和文件路径是上游工作流的标签，
不代表客户端已经安装它们；只使用本次请求实际声明的工具与真实返回。
没有工具时直接回答，不编造工具调用、文件、权限、持久化或验证结果。
保留客户端消息、图片、工具定义、调用关联及要求的输出格式；
不修改客户端配置，也不宣称本策略替换了服务端的指令层级。

"""
API_WORKFLOW = """## API 交付三拍

第一步 确认任务
从当前请求确定用户目标、上下文、输出格式和完成条件；只使用已提供的信息。

第二步 完成交付
按当前可用工具和权限执行任务；没有工具时直接给出可使用的回答、代码或步骤。
本段是请求正文，不需要安装技能、修改客户端配置或读取本机技能文件。

第三步 核对结果
区分已执行的验证与尚未验证的内容；涉及文件修改时保留备份并说明恢复方式。
按用户要求交付结果，收口：当前:对象 / 结果 / 下一步
"""


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def selected(name):
    return name in REFERENCE_FILES or (name.startswith(PACKS) and name.endswith(".md"))


def strip_installation(text, template_id):
    if text.startswith("---\n"):
        end = text.find("\n---\n", 4)
        if end == -1:
            raise ValueError(f"Invalid frontmatter: {template_id}")
        text = text[end + 5:].lstrip("\n")
    if template_id in INSTALLER_SEATS:
        text, count = re.subn(r"^安装[^\n]+\n\n", "", text, count=1, flags=re.MULTILINE)
        if count != 1:
            raise ValueError(f"Expected one installation paragraph: {template_id}")
        if template_id in {"cursor", "mimo", "kimi"}:
            text, count = re.subn(r"^(?:规则文件|技能文件)：[^\n]+\n", "", text, count=1, flags=re.MULTILINE)
            if count != 1:
                raise ValueError(f"Expected one installation file label: {template_id}")
    return text


def render(inputs, template_id):
    def read(name):
        return inputs[PACKS + name].decode("utf-8").rstrip()

    is_variant = template_id in VARIANTS
    relative = f"local-agent-skills/{template_id}.md" if is_variant else f"{template_id}.md"
    text = strip_installation(read(relative), template_id)
    labels = seat_labels(inputs)
    if set(labels) != set(SEATS):
        raise ValueError("Upstream seat inventory differs; review new or removed templates")
    if template_id == "codex":
        text = refresh_codex_header(text, inputs)
    if template_id == "opencode":
        text, count = re.subn(r"^这是随冷咖啡工作台安装到 OpenCode[^\n]+$", "",
                             text, flags=re.MULTILINE)
        if count != 1:
            raise ValueError("Expected one OpenCode installation paragraph")
    replacements = {"ACTIVATION_REPLY": read("activation-reply.md")}
    if not is_variant:
        leaves = sorted(name.removeprefix(PACKS) for name in inputs
                        if name.startswith(PACKS + "routes/leaves/")
                        and name.endswith(".md") and not name.endswith("/INDEX.md"))
        if len(leaves) != 80:
            raise ValueError("Expected all 80 pinned route leaves")
        router = "\n\n".join(read(name) for name in [
            "routes/ROUTER.md", "routes/NORMALIZER.md",
            *(f"routes/{route}.md" for route in ROUTES), "routes/leaves/INDEX.md", *leaves,
        ])
        replacements.update({
            "APP_TITLE": "ZakoAI · 冷咖啡策略", "ACTIVATION_WORD": "冷咖啡",
            "CONTROL_WORD": "BREAK//OPEN", "SEAT_ID": template_id,
            "SEAT_TAG": labels[template_id], "SEAT_NAME": labels[template_id],
            "WORKFLOW": API_WORKFLOW.rstrip() if template_id in INSTALLER_SEATS
            else read(f"workflows/{template_id}.md"),
            "ROUTER": router, "IDA_MCP": "", "UNIFIED": "",
        })
    for key, value in replacements.items():
        text = text.replace("{{" + key + "}}", value)
    if re.search(r"\{\{[^{}]*\}\}", text):
        raise ValueError(f"Unresolved upstream template: {template_id}")
    adapter = GPT_ADAPTER if template_id == "codex" else ADAPTER
    return (adapter + text.rstrip() + "\n").encode("utf-8")


def load_inputs(root, checkout=None):
    vendor = root / "third_party/coldcoffee-full"
    if checkout is not None:
        def git(*args):
            return subprocess.check_output(["git", "-C", str(checkout), *args])
        actual = git("rev-parse", "HEAD").decode().strip()
        if actual != REVISION:
            raise ValueError(f"Expected upstream {REVISION}, got {actual}")
        if git("status", "--porcelain").strip():
            raise ValueError("Upstream checkout must be clean")
        names = git("ls-tree", "-r", "--name-only", REVISION).decode().splitlines()
        return {name: git("show", f"{REVISION}:{name}") for name in sorted(names) if selected(name)}
    source = json.loads((root / "profiles/coldcoffee-full/SOURCE.json").read_text(encoding="utf-8"))
    if source["repository"] != REPOSITORY or source["revision"] != REVISION:
        raise ValueError("Bundled upstream identity does not match the pin")
    names = sorted(p.relative_to(vendor).as_posix() for p in vendor.rglob("*") if p.is_file())
    if set(names) != set(source["inputs"]) or not all(selected(name) for name in names):
        raise ValueError("Bundled upstream file inventory differs")
    inputs = {name: (vendor / name).read_bytes() for name in names}
    for name, data in inputs.items():
        if sha256(data) != source["inputs"][name]["sha256"] or len(data) != source["inputs"][name]["bytes"]:
            raise ValueError(f"Bundled upstream hash mismatch: {name}")
    return inputs


def import_profiles(root, checkout=None, check=False):
    target = root / "profiles/coldcoffee-full"
    # Git's text eol rules normalize the snapshot to LF in commits. Normalize
    # local CRLF checkouts too so the immutable content hash is platform-stable.
    legacy = (target / "legacy.json").read_bytes().replace(b"\r\n", b"\n")
    if sha256(legacy) != LEGACY_SHA256:
        raise ValueError("Historical profile snapshot hash mismatch; legacy.json must not change")
    entries = json.loads(legacy)
    if len(entries) != 42 or len({item["id"] for item in entries}) != 42:
        raise ValueError("Expected all 42 unique historical profiles")
    inputs = load_inputs(root, checkout)
    outputs = {f"{template_id}.md": render(inputs, template_id) for template_id in [*SEATS, *VARIANTS]}
    labels = {**seat_labels(inputs), **VARIANTS}
    # Fail closed if an upstream variant is added without a catalog entry.
    variants = {name.removeprefix(PACKS + "local-agent-skills/").removesuffix(".md")
                for name in inputs if name.startswith(PACKS + "local-agent-skills/")}
    if variants != set(VARIANTS):
        raise ValueError("Upstream local Agent inventory differs")
    templates = [{
        "id": template_id, "name": f"冷咖啡 · {name}",
        "source_path": PACKS + (f"local-agent-skills/{template_id}.md" if template_id in VARIANTS else f"{template_id}.md"),
    } for template_id, name in labels.items()]
    outputs["templates.json"] = json_bytes(templates)
    # Explicit project model IDs, not inferred availability from upstream labels.
    outputs["models.json"] = json_bytes([{
        "id": f"coldcoffee-{model}-v1",
        "name": f"冷咖啡 · {labels[seat]} · {model}",
        "models": [model], "model_family": family,
    } for model, family, seat in MODEL_PRESETS])
    source = {
        "repository": REPOSITORY, "revision": REVISION,
        "license": "MIT", "copyright": "Copyright (c) 2026 3641397194-wq",
        "validation": "unverified",
        "adaptations": [
            "Import all 11 seats and 5 local Agent workflows as inert API request text; execute no upstream code",
            "Inline the shared activation reply, workflows, router, normalizer and 80 leaves where requested by each seat",
            "Remove seat/variant installation frontmatter, installation paragraphs and target-file labels",
            "Replace installer-centric workflows for doubao, workbuddy, cursor, mimo and kimi with API delivery guidance",
            "Omit installer IDA/MCP and commercial unified-work blocks; preserve their upstream source for provenance",
            "Regenerate every active template from the pinned source with truthful API capability guidance",
            "Read all current seat display labels from upstream metadata; align the stale raw Codex header with its current GPT-6.2 label",
            "Retain GPT-6.2 exact presets; add GLM-5.4, Claude Fable 5.2 and Gemini 4 family exact presets from upstream relay recommendations",
            "Display-only Kimi K3.1 Pro, MiniMax M3.1 Flash and Doubao 2.1 labels remain templates; callers bind actual provider model IDs",
            "Preserve all 42 historical active profiles in immutable legacy.json for exact-match configuration migration",
        ],
        "inputs": {name: {"bytes": len(data), "sha256": sha256(data)} for name, data in sorted(inputs.items())},
        "outputs": {name: {"bytes": len(data), "sha256": sha256(data)} for name, data in sorted(outputs.items())},
        "legacy": {"file": "legacy.json", "profiles": 42, "bytes": len(legacy), "sha256": LEGACY_SHA256},
    }
    outputs["SOURCE.json"] = json_bytes(source)
    planned = {**{root / "third_party/coldcoffee-full" / name: data for name, data in inputs.items()},
               **{target / name: data for name, data in outputs.items()}}
    for path, data in planned.items():
        if check:
            if not path.exists() or path.read_bytes() != data:
                raise ValueError(f"Generated file differs: {path}")
        else:
            atomic_write(path, data)
    print(f"{'Verified' if check else 'Imported'} {len(templates)} templates from {REVISION}; 42 legacy profiles preserved; {len(MODEL_PRESETS)} exact model presets")
    print(f"Codex: {len(outputs['codex.md'])} bytes, SHA256 {sha256(outputs['codex.md'])}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", nargs="?", type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    import_profiles(Path(__file__).resolve().parents[1], args.checkout, args.check)
