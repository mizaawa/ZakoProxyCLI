use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use zakoai_proxy::{profiles, store::Group};

#[test]
fn kimi_catalog_matches_only_selected_exact_models() {
    let expected = BTreeSet::from(["kimi-k3", "kimi-k2.7", "kimi-k2.7-code", "kimi-k2.6"]);
    let actual: BTreeSet<_> = profiles::all()
        .iter()
        .filter(|profile| profile.model_family == "kimi")
        .flat_map(|profile| profile.models.iter().map(String::as_str))
        .collect();
    assert_eq!(actual, expected);
    for model in expected {
        let profile = profiles::find(&format!("kimi-break-{model}-v1")).unwrap();
        assert_eq!(profile.models, [model]);
        assert_eq!(profile.model_family, "kimi");
        let group = Group {
            profile_ids: vec![profile.id.clone()],
            ..Default::default()
        };
        assert_eq!(
            profiles::instruction(&group, model),
            Some(profile.instruction.as_str())
        );
        assert!(profiles::instruction(&group, &format!("{model}-unlisted")).is_none());
        assert!(profiles::instruction(&group, &model.to_uppercase()).is_none());
        assert!(profiles::instruction(&Group::default(), model).is_none());
        assert!(!profile.instruction.contains("${"));
        assert!(profile.instruction.contains("ZakoAI · 冷咖啡 API 内联适配"));
        assert!(
            profile
                .instruction
                .contains("只使用本次请求实际声明的工具与真实返回")
        );
        assert!(!profile.instruction.contains("running on WREN's computer"));
    }
}

#[test]
fn kimi_source_and_generated_files_match_pinned_hashes() {
    let source: Value =
        serde_json::from_str(include_str!("../profiles/kimi-break/SOURCE.json")).unwrap();
    assert_eq!(source["repository"], "https://github.com/YD-233/kimi-break");
    assert_eq!(
        source["revision"],
        "c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6"
    );
    assert_eq!(source["license"], "MIT");
    for (name, data) in [
        (
            "README.md",
            include_bytes!("../third_party/kimi-break/README.md").as_slice(),
        ),
        (
            "SYSTEM.md",
            include_bytes!("../third_party/kimi-break/SYSTEM.md").as_slice(),
        ),
        (
            "LICENSE",
            include_bytes!("../third_party/kimi-break/LICENSE").as_slice(),
        ),
    ] {
        assert_eq!(hex::encode(Sha256::digest(data)), source["inputs"][name]);
    }
    for (name, data) in [
        (
            "system.md",
            include_bytes!("../profiles/kimi-break/system.md").as_slice(),
        ),
        (
            "index.json",
            include_bytes!("../profiles/kimi-break/index.json").as_slice(),
        ),
    ] {
        assert_eq!(
            hex::encode(Sha256::digest(data)),
            source["outputs"][name]["sha256"]
        );
        assert_eq!(
            data.len() as u64,
            source["outputs"][name]["bytes"].as_u64().unwrap()
        );
    }
    let upstream = include_str!("../third_party/kimi-break/SYSTEM.md");
    let adapted = include_str!("../profiles/kimi-break/system.md");
    for (start, end) in [
        ("## Inner Voice", "# Tool use"),
        ("# Coding", "# Environment"),
        ("## user_style", ""),
    ] {
        let section = upstream.split_once(start).unwrap().1;
        let section = if end.is_empty() {
            section
        } else {
            section.split_once(end).unwrap().0
        };
        assert!(
            adapted.contains(section),
            "upstream strategy changed at {start}"
        );
    }
}
