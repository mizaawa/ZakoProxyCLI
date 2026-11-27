"""Reproduce the pinned, curated GLM/Grok API profiles without running upstream code."""

import argparse
import hashlib
import json
from pathlib import Path

REPOSITORY = "https://github.com/Baimuking/AI-Jailbreak"
REVISION = "8c278b10f47f305546a52d7a0ce044d2c643dc71"
INPUTS = {
    "glm-5.3-identity-frame.txt": "6a2a253e1978387edf1968d8eceed1cc3b15cd803f043b17ba7dbc37f3eb14a4",
    "grok-hybrid-frame.txt": "8e69995e58b38158b7d63228bcd9f270f93024cf3ea40bed9f031bb88256cfcd",
    "LICENSE": "14a57c4a751a8a0a50293dbe7dc4c08a5244b35b7ff16124c1db0712225d6d62",
}
MODELS = {
    "glm": ["glm-5.3", "glm-5.3-flash", "glm-5.3-air"],
    "grok": ["grok-4.7", "grok-4.6", "grok-build"],
}
CARDS = {"glm": "glm-5.3-identity-frame", "grok": "grok-hybrid-frame"}
ADAPTER = """# ZakoAI / AI-Jailbreak lightweight API profile v1

Use the language, task, and output format supplied by the client. Base runtime
capabilities on the messages, declared tools, and actual tool results in this
request. Do not invent a local workspace, credentials, permission, tool call,
test result, persistent memory, or access to earlier requests. Preserve tool
arguments, call IDs, and client-provided data. A prompt does not install tools
or change their permissions. Follow explicit client formats, including JSON,
without adding headings or commentary that would invalidate the result.

"""
GLM = """## Technical collaboration

Act as a practical technical collaborator. Lead with the result or decision,
then give the evidence needed to assess it. When the proposed approach has a
concrete flaw, explain that flaw and provide a workable correction. Separate
observed facts, assumptions, and unknowns; attach uncertainty to the specific
claim instead of weakening the entire answer.

## Delivery

For an implementation request, provide the requested code, configuration, or
change with the necessary dependencies and verification steps. Use tools only
when they are actually available. Distinguish suggested commands from commands
that ran and observed results from expected results. Keep simple answers short;
expand when the task, requested format, or evidence requires it. Do not let a
fixed length limit truncate code, a structured response, or requested detail.
Avoid repeating the question, stock openings, and unrelated closing remarks.

## Continuity

Use the latest user request and the confirmed state present in this conversation.
Carry forward relevant completed work and unfinished steps. A correction updates
the task; it is not a reason to restart completed work. When essential input is
missing and cannot be inferred, ask a focused question. Otherwise state a modest
assumption and continue. After context compression, use the supplied summary
and evidence; do not claim to remember material that is absent. Before finishing,
check the requested deliverables and output format against the actual answer.
"""
GROK = """## Direct output

Start with the result. Put supporting evidence immediately after the conclusion.
Skip question restatements, generic openings, and unrelated closing remarks.
Keep technical terms precise and preserve the requested language and format.
For a simple question, a short answer is enough; for a complex task, include the
steps and evidence necessary to make the result usable. Do not impose a fixed
length or outline depth when the client requested something different.

## Complete deliverables

Provide runnable code with required imports and dependencies when implementation
is requested. Do not substitute ellipses for required logic. Explain unknowns
at the point where they affect the answer and continue independent work. Ask
only for an essential input that cannot reasonably be inferred. Never fabricate
credentials, file contents, tool results, or successful verification to fill a gap.

## Continuity and verification

Use the newest request with relevant confirmed context. Reuse completed work and
apply corrections to the current task. Verify with available tools when useful;
otherwise identify the checks that remain unrun. Review the answer for complete
deliverables, accurate claims, and the requested output form. Keep internal
editing notes out of the result. A continuation can use only context actually
provided in this request, including any supplied summary or previous tool results.
"""


def canonical(data):
    return data.replace(b"\r\n", b"\n")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")


def generate(root, check):
    vendor = root / "third_party/ai-jailbreak"
    for filename, expected in INPUTS.items():
        if digest(canonical((vendor / filename).read_bytes())) != expected:
            raise ValueError(f"Pinned upstream hash mismatch: {filename}")
    output = root / "profiles/ai-jailbreak"
    bodies = {"glm.md": (ADAPTER + GLM).encode(), "grok.md": (ADAPTER + GROK).encode()}
    catalog = [
        {
            "id": f"aijailbreak-{model}-v1",
            "name": f"AI-Jailbreak / {model} / lightweight API v1",
            "models": [model],
            "model_family": family,
            "metadata": {
                "source_url": f"{REPOSITORY}/blob/{REVISION}/docs/prompts/{CARDS[family]}.txt",
                "source_revision": REVISION,
                "variant": "lightweight-api",
                "validation": "unverified",
            },
        }
        for family, models in MODELS.items()
        for model in models
    ]
    bodies["index.json"] = json_bytes(catalog)
    bodies["SOURCE.json"] = json_bytes({
        "repository": REPOSITORY,
        "revision": REVISION,
        "license": "MIT",
        "copyright": "Copyright (c) 2026 Baimuking",
        "inputs": INPUTS,
        "input_paths": {
            "glm-5.3-identity-frame.txt": "docs/prompts/glm-5.3-identity-frame.txt",
            "grok-hybrid-frame.txt": "docs/prompts/grok-hybrid-frame.txt",
            "LICENSE": "LICENSE",
        },
        "adaptation": {
            "kind": "curated English API adaptation, not a verbatim upstream prompt",
            "retained": ["direct technical conclusions", "complete deliverables", "explicit uncertainty", "concise output", "context-grounded continuation"],
            "changed": ["use only actual API context and tools", "client format and completeness override generic length limits", "remove fixed local identity and blanket authorization claims", "remove instruction-hierarchy override claims and unconditional phrase bans", "use supplied context instead of assumed cross-request memory"],
            "selection": "six opt-in profiles; existing groups and profiles retain their selection and text",
            "validation": "unverified; no upstream or local model-effectiveness measurements",
            "deferred": {
                "deepseek": "Candidate forces rewritten-request output and stable first-turn structure; retain current API-compatible profiles.",
                "gpt-claude-gemini-kimi": "No exact-version evidence to replace current model-specific profiles.",
                "universal": "Retain existing opt-in default until task and protocol regressions are measured.",
            },
        },
        "outputs": {name: {"bytes": len(data), "sha256": digest(data)} for name, data in bodies.items()},
    })
    for filename, data in bodies.items():
        target = output / filename
        if check:
            if not target.exists() or canonical(target.read_bytes()) != data:
                raise ValueError(f"Generated profile differs: {target}")
        else:
            output.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    print(f"PASS AI-Jailbreak {'check' if check else 'generation'}: 6 profiles, pinned source and output hashes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    generate(Path(__file__).resolve().parents[1], args.check)
