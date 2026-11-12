use crate::store::Group;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::OnceLock;

const DEEPSEEK_API_ADAPTER: &str = include_str!("../profiles/dsh-purge/api-adapter.md");

#[derive(Clone, Deserialize, Serialize)]
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

#[derive(Clone, Deserialize, Serialize)]
pub struct ProfileMetadata {
    pub source_url: String,
    pub source_revision: String,
    pub variant: String,
    pub validation: String,
}

const COLDCOFFEE_REVISION: &str = "3249afce6b853ff2f2133e016b223db2cf38b087";

#[derive(Deserialize, Serialize)]
pub struct ProfileTemplate {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing)]
    source_path: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_revision: String,
    #[serde(default)]
    pub validation: String,
    #[serde(skip)]
    instruction: String,
    #[serde(skip)]
    deepseek_instruction: String,
}

pub fn templates() -> &'static [ProfileTemplate] {
    static TEMPLATES: OnceLock<Vec<ProfileTemplate>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        let mut templates: Vec<ProfileTemplate> =
            serde_json::from_str(include_str!("../profiles/coldcoffee-full/templates.json"))
                .expect("Embedded Coldcoffee templates must be valid JSON");
        for template in &mut templates {
            template.instruction = match template.id.as_str() {
                "codex" => include_str!("../profiles/coldcoffee-full/codex.md"),
                "claude" => include_str!("../profiles/coldcoffee-full/claude.md"),
                "grok" => include_str!("../profiles/coldcoffee-full/grok.md"),
                "deepseek" => include_str!("../profiles/coldcoffee-full/deepseek.md"),
                "glm53" => include_str!("../profiles/coldcoffee-full/glm53.md"),
                "gemini" => include_str!("../profiles/coldcoffee-full/gemini.md"),
                "doubao" => include_str!("../profiles/coldcoffee-full/doubao.md"),
                "workbuddy" => include_str!("../profiles/coldcoffee-full/workbuddy.md"),
                "cursor" => include_str!("../profiles/coldcoffee-full/cursor.md"),
                "mimo" => include_str!("../profiles/coldcoffee-full/mimo.md"),
                "kimi" => include_str!("../profiles/coldcoffee-full/kimi.md"),
                "grok-bot" => include_str!("../profiles/coldcoffee-full/grok-bot.md"),
                "claude-duet" => include_str!("../profiles/coldcoffee-full/claude-duet.md"),
                "chatgpt-dot" => include_str!("../profiles/coldcoffee-full/chatgpt-dot.md"),
                "meta-muse" => include_str!("../profiles/coldcoffee-full/meta-muse.md"),
                _ => panic!("Unsupported Coldcoffee template"),
            }
            .into();
            template.deepseek_instruction =
                format!("{}\n\n{}", template.instruction, DEEPSEEK_API_ADAPTER);
            template.source_url = format!(
                "https://github.com/3641397194-wq/gpt6-Astra/blob/{COLDCOFFEE_REVISION}/{}",
                template.source_path
            );
            template.source_revision = COLDCOFFEE_REVISION.into();
            template.validation = "unverified".into();
        }
        templates
    })
}

fn template_metadata(template: &ProfileTemplate) -> ProfileMetadata {
    ProfileMetadata {
        source_url: template.source_url.clone(),
        source_revision: template.source_revision.clone(),
        variant: template.id.clone(),
        validation: template.validation.clone(),
    }
}

fn bound_template(id: &str) -> Option<(&'static ProfileTemplate, String)> {
    let (template_id, encoded) = id.strip_prefix("coldcoffee@")?.split_once('@')?;
    if encoded.is_empty()
        || encoded.len() > 1024
        || encoded.len() % 2 != 0
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let model = String::from_utf8(hex::decode(encoded).ok()?).ok()?;
    if model.trim() != model || model.chars().any(char::is_control) {
        return None;
    }
    let template = templates()
        .iter()
        .find(|template| template.id == template_id)?;
    Some((template, model))
}

fn template_instruction(template: &'static ProfileTemplate, model: &str) -> &'static str {
    if model_family(model) == Some("deepseek") {
        &template.deepseek_instruction
    } else {
        &template.instruction
    }
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
            let seat = match family {
                "gpt-5.6" | "gpt-6" | "gpt-6.1" => "codex",
                "glm" => "glm53",
                other => other,
            };
            let template = templates()
                .iter()
                .find(|template| template.id == seat)
                .expect("Every historical model family has a Coldcoffee template");
            profile.instruction = template.instruction.clone();
            profile.name = format!("{} · {}", template.name, profile.models[0]);
            profile.metadata = Some(template_metadata(template));
            if family == "deepseek" {
                profile.instruction.push_str("\n\n");
                profile.instruction.push_str(DEEPSEEK_API_ADAPTER);
            }
            profile.model_family = family.into();
        }
        profiles
    })
}

pub fn find(id: &str) -> Option<Cow<'static, Profile>> {
    if let Some(profile) = all().iter().find(|profile| profile.id == id) {
        return Some(Cow::Borrowed(profile));
    }
    let (template, model) = bound_template(id)?;
    Some(Cow::Owned(Profile {
        id: id.into(),
        name: format!("{} · {model}", template.name),
        instruction: template_instruction(template, &model).into(),
        model_family: model_family(&model).unwrap_or("custom").into(),
        models: vec![model],
        metadata: Some(template_metadata(template)),
    }))
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
    current
        .or_else(|| {
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
                .and_then(|profile| all().iter().find(|current| current.id == profile.id))
        })
        .or_else(|| {
            static LEGACY: OnceLock<Vec<Profile>> = OnceLock::new();
            LEGACY
                .get_or_init(|| {
                    serde_json::from_str(include_str!("../profiles/coldcoffee-full/legacy.json"))
                        .expect("Historical full catalog must be valid JSON")
                })
                .iter()
                .find(|profile| {
                    let without_adapter = profile
                        .instruction
                        .strip_suffix(DEEPSEEK_API_ADAPTER)
                        .and_then(|text| text.strip_suffix("\n\n"));
                    (instruction == profile.instruction || without_adapter == Some(instruction))
                        && models.len() == profile.models.len()
                        && models.iter().all(|model| profile.models.contains(model))
                })
                .and_then(|profile| all().iter().find(|current| current.id == profile.id))
        })
}

pub fn instruction(group: &Group, model: &str) -> Option<&'static str> {
    group
        .profile_ids
        .iter()
        .find_map(|id| {
            if let Some(profile) = all().iter().find(|profile| profile.id == *id) {
                return profile
                    .models
                    .iter()
                    .any(|selected| selected == model)
                    .then_some(profile.instruction.as_str());
            }
            let (template, bound_model) = bound_template(id)?;
            (bound_model == model).then(|| template_instruction(template, model))
        })
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
    fn template_bindings_are_exact_canonical_and_do_not_guess_model_names() {
        assert_eq!(templates().len(), 15);
        let model = "provider/实际模型";
        for template in templates() {
            let id = format!("coldcoffee@{}@{}", template.id, hex::encode(model));
            let profile = find(&id).unwrap();
            assert_eq!(profile.models, [model]);
            let group = Group {
                profile_ids: vec![id],
                ..Default::default()
            };
            assert_eq!(
                instruction(&group, model),
                Some(profile.instruction.as_str())
            );
            assert!(instruction(&group, "provider/另一个模型").is_none());
            assert!(instruction(&Group::default(), model).is_none());
            assert!(!profile.instruction.contains("{{"));
            assert_eq!(
                profile.metadata.as_ref().unwrap().source_revision,
                COLDCOFFEE_REVISION
            );
        }
        for invalid in [
            "coldcoffee@unknown@61",
            "coldcoffee@kimi@",
            "coldcoffee@kimi@ff",
            "coldcoffee@kimi@0A",
            "coldcoffee@kimi@0a",
            "coldcoffee@kimi@20",
            "coldcoffee@kimi@616",
            "coldcoffee@kimi@zz",
            "coldcoffee@kimi@61@62",
        ] {
            assert!(find(invalid).is_none(), "{invalid}");
        }
        assert!(find(&format!("coldcoffee@kimi@{}", "61".repeat(513))).is_none());
        let id = format!("coldcoffee@kimi@{}", hex::encode("deepseek-flash"));
        assert_eq!(
            find(&id)
                .unwrap()
                .instruction
                .matches("# DeepSeek API request adapter")
                .count(),
            1
        );
    }

    #[test]
    fn all_historical_selections_and_raw_presets_keep_equivalent_latest_behavior() {
        let historical: Vec<Profile> =
            serde_json::from_str(include_str!("../profiles/coldcoffee-full/legacy.json")).unwrap();
        assert_eq!(historical.len(), all().len());
        for old in historical {
            let current = find(&old.id).unwrap();
            assert_eq!(current.models, old.models);
            let migrated = legacy_profile(&old.models, &old.instruction).unwrap();
            assert_eq!(migrated.models, current.models);
            assert_eq!(migrated.instruction, current.instruction);
            assert!(legacy_profile(&old.models, &format!("{} custom", old.instruction)).is_none());
            assert_eq!(
                current.metadata.as_ref().unwrap().source_revision,
                COLDCOFFEE_REVISION
            );
        }
    }

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
                        include_str!("../profiles/coldcoffee-full/deepseek.md"),
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
            assert_eq!(
                metadata.variant,
                if model.starts_with("glm") {
                    "glm53"
                } else {
                    "grok"
                }
            );
            assert_eq!(metadata.validation, "unverified");
            assert_eq!(metadata.source_revision, COLDCOFFEE_REVISION);
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
            (
                "glm-5.3",
                include_str!("../profiles/coldcoffee-full/glm53.md"),
            ),
            (
                "grok-4.7",
                include_str!("../profiles/coldcoffee-full/grok.md"),
            ),
        ] {
            let legacy = find(&format!("coldcoffee-{model}-v1")).unwrap();
            assert_eq!(legacy.instruction, original);
            assert_eq!(
                legacy.metadata.as_ref().unwrap().source_revision,
                COLDCOFFEE_REVISION
            );
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
            assert_eq!(metadata.variant, "codex");
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
                instruction(&group, &profile.models[0]),
                Some(profile.instruction.as_str())
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
