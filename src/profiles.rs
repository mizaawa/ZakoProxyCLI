use crate::store::Group;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const DEEPSEEK_API_ADAPTER: &str = include_str!("../profiles/dsh-purge/api-adapter.md");

#[derive(Deserialize, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub model_family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ProfileMetadata>,
}

#[derive(Deserialize, Serialize)]
pub struct ProfileMetadata {
    pub source_url: String,
    pub source_revision: String,
    pub variant: String,
    pub validation: String,
}

pub fn model_family(model: &str) -> Option<&'static str> {
    [
        "gpt-5.6", "gpt-6.1", "gpt-6", "claude", "grok", "gemini", "deepseek", "glm", "kimi",
    ]
    .into_iter()
    .find(|family| {
        model == *family
            || model
                .strip_prefix(*family)
                .is_some_and(|suffix| suffix.starts_with('-'))
    })
}

pub fn all() -> &'static [Profile] {
    static PROFILES: OnceLock<Vec<Profile>> = OnceLock::new();
    PROFILES.get_or_init(|| {
        let mut profiles: Vec<Profile> =
            serde_json::from_str(include_str!("../profiles/index.json"))
                .expect("Embedded profiles must be valid JSON");
        let coldcoffee: Vec<Profile> =
            serde_json::from_str(include_str!("../profiles/coldcoffee/index.json"))
                .expect("Embedded Coldcoffee catalog must be valid JSON");
        for mut profile in coldcoffee {
            profile.instruction = match profile.model_family.as_str() {
                "claude" => include_str!("../profiles/coldcoffee/claude.md"),
                "grok" => include_str!("../profiles/coldcoffee/grok.md"),
                "gemini" => include_str!("../profiles/coldcoffee/gemini.md"),
                "deepseek" => include_str!("../profiles/coldcoffee/deepseek.md"),
                "glm" => include_str!("../profiles/coldcoffee/glm53.md"),
                _ => panic!("Unsupported Coldcoffee model family"),
            }
            .into();
            // Preserve the historical ID so already-selected groups keep their choice.
            if profile.id == "coldcoffee-deepseek-v4.1-flash-v1" {
                profile.name = "hanshuang · deepseek-v4.1-flash".into();
                profile.models.push("deepseek-4.1-flash".into());
                profile.instruction = include_str!("../profiles/hanshuang/system.md").into();
            }
            profiles.push(profile);
        }
        let hanshuang: Vec<Profile> =
            serde_json::from_str(include_str!("../profiles/hanshuang/index.json"))
                .expect("Embedded Hanshuang catalog must be valid JSON");
        for mut profile in hanshuang {
            profile.instruction = include_str!("../profiles/hanshuang/system.md").into();
            profiles.push(profile);
        }
        let kimi: Vec<Profile> =
            serde_json::from_str(include_str!("../profiles/kimi-break/index.json"))
                .expect("Embedded kimi-break catalog must be valid JSON");
        for mut profile in kimi {
            profile.instruction = include_str!("../profiles/kimi-break/system.md").into();
            profiles.push(profile);
        }
        let ai_jailbreak: Vec<Profile> =
            serde_json::from_str(include_str!("../profiles/ai-jailbreak/index.json"))
                .expect("Embedded AI-Jailbreak catalog must be valid JSON");
        for mut profile in ai_jailbreak {
            profile.instruction = match profile.model_family.as_str() {
                "glm" => include_str!("../profiles/ai-jailbreak/glm.md"),
                "grok" => include_str!("../profiles/ai-jailbreak/grok.md"),
                _ => panic!("Unsupported AI-Jailbreak model family"),
            }
            .into();
            profiles.push(profile);
        }
        let mut ids = std::collections::HashSet::new();
        for profile in &mut profiles {
            assert!(ids.insert(profile.id.clone()), "Profile IDs must be unique");
            let family = profile
                .models
                .first()
                .and_then(|model| model_family(model))
                .expect("Embedded profiles must declare a supported model family");
            assert!(
                profile
                    .models
                    .iter()
                    .all(|model| model_family(model) == Some(family)),
                "A profile must target exactly one model family"
            );
            assert!(!profile.instruction.is_empty());
            if family == "deepseek" {
                profile.instruction.push_str("\n\n");
                profile.instruction.push_str(DEEPSEEK_API_ADAPTER);
            }
            profile.model_family = family.into();
        }
        profiles
    })
}

pub fn find(id: &str) -> Option<&'static Profile> {
    all().iter().find(|profile| profile.id == id)
}

// Only exact historical presets migrate; discarded custom fields never become active.
pub(crate) fn legacy_profile(models: &[String], instruction: &str) -> Option<&'static Profile> {
    let current = all().iter().find(|profile| {
        (instruction == profile.instruction
            || (profile.model_family == "deepseek"
                && profile
                    .instruction
                    .strip_suffix(DEEPSEEK_API_ADAPTER)
                    .and_then(|previous| previous.strip_suffix("\n\n"))
                    .is_some_and(|previous| instruction == previous)))
            && models.len() == profile.models.len()
            && models.iter().all(|model| profile.models.contains(model))
    });
    current.or_else(|| {
        static LEGACY_GPT: OnceLock<Vec<Profile>> = OnceLock::new();
        LEGACY_GPT
            .get_or_init(|| {
                serde_json::from_str(include_str!(
                    "../third_party/gpt-instruct/profiles.legacy.json"
                ))
                .expect("Historical GPT profiles must be valid JSON")
            })
            .iter()
            .find(|profile| {
                instruction == profile.instruction
                    && models.len() == profile.models.len()
                    && models.iter().all(|model| profile.models.contains(model))
            })
            .and_then(|profile| find(&profile.id))
    })
}

pub fn instruction(group: &Group, model: &str) -> Option<&'static str> {
    group
        .profile_ids
        .iter()
        .filter_map(|id| find(id))
        .find(|profile| profile.models.iter().any(|selected| selected == model))
        .map(|profile| profile.instruction.as_str())
        .or_else(|| {
            group
                .default_strategy_enabled
                .then_some(DEFAULT_INSTRUCTION)
        })
}

/// Opt-in fallback, applied only when no selected exact-model profile matches.
pub const DEFAULT_INSTRUCTION: &str = include_str!("../profiles/default.md");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_have_exact_or_hyphen_boundaries() {
        for (model, expected) in [
            ("gpt-5.6", Some("gpt-5.6")),
            ("gpt-5.6-sol", Some("gpt-5.6")),
            ("gpt-6", Some("gpt-6")),
            ("gpt-6-astra", Some("gpt-6")),
            ("gpt-6-sol", Some("gpt-6")),
            ("gpt-6.1", Some("gpt-6.1")),
            ("gpt-6.1-sol", Some("gpt-6.1")),
            ("gpt-6.10-sol", None),
            ("gpt-60", None),
            ("gpt-5.60-sol", None),
            ("other-gpt-6", None),
            ("GPT-6", None),
            ("claude-opus-5-5", Some("claude")),
            ("grok-4.7", Some("grok")),
            ("grok-4.6", Some("grok")),
            ("grok-build", Some("grok")),
            ("gemini-3.1-pro-preview", Some("gemini")),
            ("deepseek-v4.1-flash", Some("deepseek")),
            ("glm-5.3", Some("glm")),
            ("glm-5.3-flash", Some("glm")),
            ("glm-5.3-air", Some("glm")),
            ("kimi-k2.7-code", Some("kimi")),
            ("kimilike-k3", None),
            ("claudelike-5", None),
            ("grokish", None),
            ("Gemini-3.1-pro", None),
        ] {
            assert_eq!(model_family(model), expected, "{model}");
        }
    }

    #[test]
    fn coldcoffee_catalog_has_exact_requested_models_and_resolved_skills() {
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../profiles/coldcoffee/index.json")).unwrap();
        assert_eq!(entries.len(), 28);
        assert_eq!(all().len(), 42);
        let mut models = std::collections::HashSet::new();
        for entry in entries {
            let profile = find(entry["id"].as_str().unwrap()).unwrap();
            let is_hanshuang = profile.id == "coldcoffee-deepseek-v4.1-flash-v1";
            assert_eq!(profile.models.len(), if is_hanshuang { 2 } else { 1 });
            let model = &profile.models[0];
            assert!(!model.contains("svg"));
            assert!(
                models.insert(model.clone()),
                "duplicate exact model {model}"
            );
            if is_hanshuang {
                assert_eq!(
                    profile.instruction,
                    format!(
                        "{}\n\n{}",
                        include_str!("../profiles/hanshuang/system.md"),
                        DEEPSEEK_API_ADAPTER
                    )
                );
                assert!(
                    profile
                        .models
                        .iter()
                        .any(|model| model == "deepseek-4.1-flash")
                );
            } else {
                assert!(profile.instruction.contains("ZakoAI · 冷咖啡 API 内联适配"));
                assert!(profile.instruction.contains("三拍"));
                assert!(profile.instruction.contains("CHA-ROUTE"));
            }
            assert!(!profile.instruction.contains("{{"));
            assert!(!profile.instruction.contains("127.0.0.1:13337"));
            let group = Group {
                profile_ids: vec![profile.id.clone()],
                ..Default::default()
            };
            assert!(instruction(&group, model).is_some());
            assert!(instruction(&group, &format!("{model}-unlisted")).is_none());
        }
        assert!(models.contains("gemini-3.1-flash-image"));
        assert!(models.contains("claude-fable-5-1"));
        assert!(models.contains("deepseek-v4-pro-0813"));
        assert!(models.contains("glm-5.3"));
    }

    #[test]
    fn ai_jailbreak_profiles_are_opt_in_exact_model_variants() {
        let expected = [
            "glm-5.3",
            "glm-5.3-flash",
            "glm-5.3-air",
            "grok-4.7",
            "grok-4.6",
            "grok-build",
        ];
        let actual: std::collections::BTreeSet<_> = all()
            .iter()
            .filter(|profile| profile.id.starts_with("aijailbreak-"))
            .flat_map(|profile| profile.models.iter().map(String::as_str))
            .collect();
        assert_eq!(actual, expected.into_iter().collect());
        for model in expected {
            let profile = find(&format!("aijailbreak-{model}-v1")).unwrap();
            assert_eq!(profile.models, [model]);
            let metadata = profile.metadata.as_ref().unwrap();
            assert_eq!(metadata.variant, "lightweight-api");
            assert_eq!(metadata.validation, "unverified");
            assert_eq!(
                metadata.source_revision,
                "8c278b10f47f305546a52d7a0ce044d2c643dc71"
            );
            assert!(metadata.source_url.contains(&metadata.source_revision));
            let group = Group {
                profile_ids: vec![profile.id.clone()],
                ..Default::default()
            };
            assert_eq!(
                instruction(&group, model),
                Some(profile.instruction.as_str())
            );
            assert!(instruction(&group, &model.to_uppercase()).is_none());
            assert!(instruction(&group, &format!("{model}-unlisted")).is_none());
            assert!(instruction(&Group::default(), model).is_none());
            let fallback = Group {
                default_strategy_enabled: true,
                ..Default::default()
            };
            assert_eq!(instruction(&fallback, model), Some(DEFAULT_INSTRUCTION));
        }
        for (model, original) in [
            ("glm-5.3", include_str!("../profiles/coldcoffee/glm53.md")),
            ("grok-4.7", include_str!("../profiles/coldcoffee/grok.md")),
        ] {
            let legacy = find(&format!("coldcoffee-{model}-v1")).unwrap();
            assert_eq!(legacy.instruction, original);
            assert!(legacy.metadata.is_none());
            let group = Group {
                profile_ids: vec![legacy.id.clone()],
                ..Default::default()
            };
            assert_eq!(instruction(&group, model), Some(original));
        }
    }

    #[test]
    fn dsh_purge_adapter_is_composed_once_for_deepseek_and_keeps_legacy_profiles() {
        use sha2::{Digest, Sha256};

        let source: serde_json::Value =
            serde_json::from_str(include_str!("../profiles/dsh-purge/SOURCE.json")).unwrap();
        assert_eq!(
            source["repository"],
            "https://github.com/YuJunZhiXue/dsh-purge"
        );
        assert_eq!(
            source["revision"],
            "6c1d638d1bdcc45ede50fc30249c753d58122a51"
        );
        let normalized_adapter = DEEPSEEK_API_ADAPTER.replace("\r\n", "\n");
        assert_eq!(
            source["outputs"]["api-adapter.md"]["sha256"],
            hex::encode(Sha256::digest(normalized_adapter.as_bytes()))
        );
        assert_eq!(
            source["outputs"]["api-adapter.md"]["bytes"],
            normalized_adapter.len()
        );

        let deepseek: Vec<_> = all()
            .iter()
            .filter(|profile| profile.model_family == "deepseek")
            .collect();
        assert_eq!(deepseek.len(), 6);
        for profile in deepseek {
            assert_eq!(
                profile
                    .instruction
                    .matches("# DeepSeek API request adapter")
                    .count(),
                1
            );
            assert!(profile.instruction.ends_with(DEEPSEEK_API_ADAPTER));
            let previous = profile
                .instruction
                .strip_suffix(DEEPSEEK_API_ADAPTER)
                .unwrap()
                .strip_suffix("\n\n")
                .unwrap();
            assert!(legacy_profile(&profile.models, previous).is_some());
            assert!(legacy_profile(&profile.models, &profile.instruction).is_some());
        }

        let kimi = find("kimi-break-kimi-k2.7-code-v1").unwrap();
        assert!(!kimi.instruction.contains("# DeepSeek API request adapter"));
        assert_eq!(
            find("coldcoffee-deepseek-v4.1-flash-v1").unwrap().models,
            ["deepseek-v4.1-flash", "deepseek-4.1-flash"]
        );
        let short_name = find("hanshuang-deepseek-flash-v1").unwrap();
        assert_eq!(short_name.models, ["deepseek-flash"]);
        assert_eq!(
            short_name.instruction,
            find("coldcoffee-deepseek-v4.1-flash-v1")
                .unwrap()
                .instruction
        );
    }

    #[test]
    fn coldcoffee_gpt_keeps_exact_model_selection_and_legacy_settings() {
        let historical: Vec<Profile> = serde_json::from_str(include_str!(
            "../third_party/gpt-instruct/profiles.legacy.json"
        ))
        .unwrap();
        assert_eq!(historical.len(), 3);
        for old in historical {
            let profile = find(&old.id).unwrap();
            assert_eq!(profile.models, old.models);
            assert_ne!(profile.instruction, old.instruction);
            assert_eq!(
                profile.instruction,
                include_str!("../profiles/coldcoffee-gpt/system.md")
            );
            assert!(!profile.instruction.contains("{{"));
            let metadata = profile.metadata.as_ref().unwrap();
            assert_eq!(metadata.variant, "codex-api-inline");
            assert_eq!(metadata.validation, "unverified");
            assert_eq!(
                metadata.source_revision,
                "3249afce6b853ff2f2133e016b223db2cf38b087"
            );
            let group: Group = serde_json::from_value(serde_json::json!({
                "name":"Historical GPT", "models":old.models, "instruction":old.instruction
            }))
            .unwrap();
            assert_eq!(group.profile_ids.as_slice(), std::slice::from_ref(&old.id));
            assert_eq!(
                instruction(&group, &old.models[0]),
                Some(profile.instruction.as_str())
            );
            assert!(instruction(&group, &format!("{}-unlisted", old.models[0])).is_none());
            assert!(instruction(&Group::default(), &old.models[0]).is_none());
            assert!(legacy_profile(&old.models, &format!("{} custom", old.instruction)).is_none());
            assert!(legacy_profile(&["gpt-60".into()], &old.instruction).is_none());
        }
    }

    #[test]
    fn legacy_migration_only_recognizes_unchanged_presets() {
        for profile in all() {
            let group: Group = serde_json::from_value(serde_json::json!({
                "name":"Legacy", "models":profile.models, "instruction":profile.instruction
            }))
            .unwrap();
            assert_eq!(
                group.profile_ids.as_slice(),
                std::slice::from_ref(&profile.id)
            );
            let serialized = serde_json::to_value(&group).unwrap();
            assert!(serialized.get("models").is_none());
            assert!(serialized.get("instruction").is_none());

            let custom: Group = serde_json::from_value(serde_json::json!({
                "name":"Custom", "models":["custom-model"], "instruction":"Custom text"
            }))
            .unwrap();
            assert!(custom.profile_ids.is_empty());
            assert!(instruction(&custom, "custom-model").is_none());
        }
    }
}
