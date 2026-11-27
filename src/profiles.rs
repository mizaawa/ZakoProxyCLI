use crate::store::Group;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap, sync::OnceLock};

const COLDCOFFEE_REVISION: &str = "a30126e7c7d6c2ffe6b08f007cbc055d9b498f71";
const DEEPSEEK_API_ADAPTER: &str = include_str!("../profiles/dsh-purge/api-adapter.md");
pub const DEFAULT_INSTRUCTION: &str = include_str!("../profiles/default.md");

#[derive(Clone, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
    pub model_family: &'static str,
    pub instruction: String,
    pub metadata: Option<ProfileMetadata>,
}

#[derive(Clone, Serialize)]
pub struct ProfileMetadata {
    pub source_name: &'static str,
    pub source_url: String,
    pub source_revision: String,
    pub variant: String,
    pub validation: &'static str,
}

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
    instruction: &'static str,
    #[serde(skip)]
    deepseek_instruction: String,
}

#[derive(Deserialize)]
struct Catalog {
    models: BTreeMap<String, Vec<String>>,
    templates: Vec<String>,
}

fn catalog() -> &'static Catalog {
    static DATA: OnceLock<Catalog> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("relay-catalog.json")).expect("invalid relay catalog")
    })
}

fn template_body(id: &str) -> Option<&'static str> {
    Some(match id {
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
        "opencode" => include_str!("../profiles/coldcoffee-full/opencode.md"),
        _ => return None,
    })
}

fn usable(text: &str) -> bool {
    !text.trim().is_empty()
        && !text.contains("{{")
        && text.contains("## API 默认启用")
        && text.matches(" · LEAF-").count() == 80
}

pub fn templates() -> &'static [ProfileTemplate] {
    static TEMPLATES: OnceLock<Vec<ProfileTemplate>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        let items: Vec<ProfileTemplate> =
            serde_json::from_str(include_str!("../profiles/coldcoffee-full/templates.json"))
                .expect("invalid template catalog");
        items
            .into_iter()
            .filter_map(|mut template| {
                let text = template_body(&template.id)?;
                if !catalog().templates.contains(&template.id) || !usable(text) {
                    return None;
                }
                template.instruction = text;
                template.deepseek_instruction = format!("{text}\n\n{DEEPSEEK_API_ADAPTER}");
                template.source_url = format!(
                    "https://github.com/3641397194-wq/gpt6-Astra/blob/{COLDCOFFEE_REVISION}/{}",
                    template.source_path
                );
                template.source_revision = COLDCOFFEE_REVISION.into();
                template.validation = "unverified".into();
                Some(template)
            })
            .collect()
    })
}

pub fn model_family(model: &str) -> Option<&'static str> {
    let lower = model.to_ascii_lowercase();
    let model = lower.rsplit('/').next()?;
    [
        "gpt-5.6",
        "gpt-6.2",
        "gpt-6.1",
        "gpt-6",
        "gpt",
        "codex",
        "o1",
        "o3",
        "o4",
        "claude-duet",
        "claude",
        "grok-bot",
        "grok",
        "gemini",
        "deepseek",
        "glm",
        "kimi",
        "doubao",
        "workbuddy",
        "cursor",
        "minimax",
        "mimo",
        "chatgpt-dot",
        "chatgpt",
        "meta-muse",
        "opencode",
    ]
    .into_iter()
    .find(|family| {
        model == *family
            || model
                .strip_prefix(family)
                .is_some_and(|s| s.starts_with('-'))
    })
}

fn automatic_template(model: &str) -> Option<&'static ProfileTemplate> {
    let seat = match model_family(model)? {
        "gpt-5.6" | "gpt-6" | "gpt-6.1" | "gpt-6.2" | "gpt" | "codex" | "o1" | "o3" | "o4"
        | "chatgpt" => "codex",
        "glm" => "glm53",
        "minimax" | "mimo" => "mimo",
        other => other,
    };
    templates().iter().find(|t| t.id == seat)
}

fn valid_bound_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 512
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

// Scope historical multi-model profiles to one checkbox without changing their source.
fn for_model(mut profile: Profile, model: &str) -> Profile {
    if profile.models.len() > 1 {
        profile.id = format!("profile@{}@{}", profile.id, hex::encode(model));
        profile.models = vec![model.into()];
    }
    profile
}

pub fn model_profile(model: &str, selected: &[String]) -> Option<Profile> {
    if !valid_bound_model(model) {
        return None;
    }
    // Existing per-key source choices win over automatic defaults.
    if let Some(profile) = selected
        .iter()
        .filter_map(|id| find(id))
        .find(|p| p.models.iter().any(|m| m == model))
    {
        return Some(for_model(profile.into_owned(), model));
    }
    if let Some(profile) = all()
        .iter()
        .filter(|p| p.models.iter().any(|m| m == model))
        .min_by_key(|p| {
            !p.metadata
                .as_ref()
                .is_some_and(|m| m.source_revision == COLDCOFFEE_REVISION)
        })
    {
        return Some(for_model(profile.clone(), model));
    }
    let template = automatic_template(model)?;
    let id = format!("coldcoffee@{}@{}", template.id, hex::encode(model));
    Some(descriptor(&id, vec![model.into()], template))
}

fn template_instruction(template: &'static ProfileTemplate, model: &str) -> &'static str {
    if model_family(model) == Some("deepseek") {
        &template.deepseek_instruction
    } else {
        template.instruction
    }
}

fn descriptor(id: &str, models: Vec<String>, template: &'static ProfileTemplate) -> Profile {
    Profile {
        id: id.into(),
        name: format!("{} · {}", template.name, models[0]),
        model_family: model_family(&models[0]).unwrap_or("custom"),
        instruction: template_instruction(template, &models[0]).into(),
        models,
        metadata: Some(ProfileMetadata {
            source_name: "冷咖啡",
            source_url: template.source_url.clone(),
            source_revision: COLDCOFFEE_REVISION.into(),
            variant: template.id.clone(),
            validation: "unverified",
        }),
    }
}

pub fn all() -> &'static [Profile] {
    static PROFILES: OnceLock<Vec<Profile>> = OnceLock::new();
    PROFILES.get_or_init(|| {
        catalog()
            .models
            .iter()
            .filter_map(|(id, models)| {
                let family = model_family(models.first()?)?;
                if !models
                    .iter()
                    .all(|model| model_family(model) == Some(family))
                {
                    return None;
                }
                if id.starts_with("gpt-")
                    || id.starts_with("hanshuang-")
                    || id.starts_with("kimi-break-")
                    || id.starts_with("aijailbreak-")
                    || id == "coldcoffee-deepseek-v4.1-flash-v1"
                {
                    return source_profile(id, models, family);
                }
                if !id.starts_with("coldcoffee-") {
                    return None;
                }
                let template = automatic_template(&models[0])?;
                Some(descriptor(id, models.clone(), template))
            })
            .collect()
    })
}

fn source_profile(id: &str, models: &[String], family: &'static str) -> Option<Profile> {
    let (name, body, source_url, revision, variant) = if id.starts_with("gpt-") {
        #[derive(Deserialize)]
        struct Entry {
            id: String,
            instruction: String,
        }
        let entries: Vec<Entry> = serde_json::from_str(include_str!(
            "../third_party/gpt-instruct/profiles.legacy.json"
        ))
        .expect("invalid gpt-instruct snapshot");
        let entry = entries.into_iter().find(|p| p.id == id)?;
        (
            "gpt-instruct",
            entry.instruction,
            "https://github.com/MDX-Tom/gpt-instruct".to_owned(),
            "historical-snapshot",
            "original-model-profile",
        )
    } else if id.starts_with("hanshuang-") || id == "coldcoffee-deepseek-v4.1-flash-v1" {
        (
            "寒霜 / Hanshuang", include_str!("../profiles/hanshuang/system.md").to_owned(),
            "https://github.com/aimeoa/hanshuang-codex/tree/cf9c1537c2bd4ed8577ed6509ee665a39709d7d4".to_owned(),
            "cf9c1537c2bd4ed8577ed6509ee665a39709d7d4", "deepseek-api",
        )
    } else if id.starts_with("kimi-break-") {
        (
            "kimi-break", include_str!("../profiles/kimi-break/system.md").to_owned(),
            "https://github.com/YD-233/kimi-break/blob/c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6/SYSTEM.md".to_owned(),
            "c6a7f6dfffd17c45e60f6eb965f1d4bc169cd8f6", "kimi-api",
        )
    } else if id.starts_with("aijailbreak-") {
        let (body, variant) = match family {
            "glm" => (
                include_str!("../profiles/ai-jailbreak/glm.md"),
                "glm-5.3-identity-frame",
            ),
            "grok" => (
                include_str!("../profiles/ai-jailbreak/grok.md"),
                "grok-hybrid-frame",
            ),
            _ => return None,
        };
        (
            "AI-Jailbreak",
            body.to_owned(),
            format!(
                "https://github.com/Baimuking/AI-Jailbreak/blob/8c278b10f47f305546a52d7a0ce044d2c643dc71/docs/prompts/{variant}.txt"
            ),
            "8c278b10f47f305546a52d7a0ce044d2c643dc71",
            variant,
        )
    } else {
        return None;
    };
    if body.trim().is_empty() || body.contains("{{") || body.contains("${") {
        return None;
    }
    let instruction = if family == "deepseek" {
        format!("{body}\n\n{DEEPSEEK_API_ADAPTER}")
    } else {
        body
    };
    Some(Profile {
        id: id.into(),
        name: format!("{name} · {}", models[0]),
        models: models.to_vec(),
        model_family: family,
        instruction,
        metadata: Some(ProfileMetadata {
            source_name: name,
            source_url,
            source_revision: revision.into(),
            variant: variant.into(),
            validation: "unverified",
        }),
    })
}

fn decode_bound_model(encoded: &str) -> Option<String> {
    if encoded.is_empty()
        || encoded.len() > 1024
        || !encoded.len().is_multiple_of(2)
        || !encoded
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let model = String::from_utf8(hex::decode(encoded).ok()?).ok()?;
    valid_bound_model(&model).then_some(model)
}

fn bound_template(id: &str) -> Option<(&'static ProfileTemplate, String)> {
    let (template, encoded) = id.strip_prefix("coldcoffee@")?.split_once('@')?;
    let model = decode_bound_model(encoded)?;
    Some((templates().iter().find(|t| t.id == template)?, model))
}

fn bound_source(id: &str) -> Option<(&'static Profile, String)> {
    let (source, encoded) = id.strip_prefix("profile@")?.split_once('@')?;
    let model = decode_bound_model(encoded)?;
    let profile = all()
        .iter()
        .find(|p| p.id == source && p.models.contains(&model))?;
    Some((profile, model))
}

pub fn find(id: &str) -> Option<Cow<'static, Profile>> {
    if let Some(profile) = all().iter().find(|p| p.id == id) {
        return Some(Cow::Borrowed(profile));
    }
    if let Some((profile, model)) = bound_source(id) {
        let mut profile = profile.clone();
        profile.id = id.into();
        profile.models = vec![model];
        return Some(Cow::Owned(profile));
    }
    let (template, model) = bound_template(id)?;
    Some(Cow::Owned(descriptor(id, vec![model], template)))
}

pub(crate) fn legacy_profile(models: &[String], text: &str) -> Option<&'static Profile> {
    let same_models = |expected: &[String]| {
        let mut expected: Vec<_> = expected.iter().collect();
        let mut actual: Vec<_> = models.iter().collect();
        expected.sort_unstable();
        actual.sort_unstable();
        actual == expected
    };
    if let Some(profile) = all().iter().find(|p| {
        same_models(&p.models)
            && (p.instruction == text
                || (p.model_family == "deepseek"
                    && p.instruction
                        .strip_suffix(DEEPSEEK_API_ADAPTER)
                        .and_then(|s| s.strip_suffix("\n\n"))
                        == Some(text)))
    }) {
        return Some(profile);
    }
    #[derive(Deserialize)]
    struct Legacy {
        id: String,
        models: Vec<String>,
        instruction: String,
    }
    static LEGACY: OnceLock<Vec<Legacy>> = OnceLock::new();
    let old = LEGACY
        .get_or_init(|| {
            let mut entries: Vec<Legacy> =
                serde_json::from_str(include_str!("../profiles/coldcoffee-full/legacy.json"))
                    .unwrap();
            entries.extend(
                serde_json::from_str::<Vec<Legacy>>(include_str!(
                    "../third_party/gpt-instruct/profiles.legacy.json"
                ))
                .unwrap(),
            );
            entries
        })
        .iter()
        .find(|p| same_models(&p.models) && p.instruction == text)?;
    all().iter().find(|p| p.id == old.id)
}

pub fn instruction(group: &Group, model: &str) -> Option<&'static str> {
    group
        .profile_ids
        .iter()
        .find_map(|id| {
            if let Some(p) = all().iter().find(|p| p.id == *id) {
                return p
                    .models
                    .iter()
                    .any(|m| m == model)
                    .then_some(p.instruction.as_str());
            }
            if let Some((profile, bound_model)) = bound_source(id) {
                return (bound_model == model).then_some(profile.instruction.as_str());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_templates_follow_model_families_and_preserve_exact_names() {
        for (model, template) in [
            ("gpt-7-test", "codex"),
            ("openai/GPT-6.2-new", "codex"),
            ("o3-test", "codex"),
            ("Claude-New", "claude"),
            ("grok-new", "grok"),
            ("gemini-new", "gemini"),
            ("deepseek-new", "deepseek"),
            ("z-ai/GLM-5.4-new", "glm53"),
            ("kimi-new", "kimi"),
            ("doubao-new", "doubao"),
            ("MiniMax-M3.1", "mimo"),
            ("workbuddy", "workbuddy"),
            ("cursor", "cursor"),
            ("grok-bot", "grok-bot"),
            ("claude-duet", "claude-duet"),
            ("chatgpt-dot", "chatgpt-dot"),
            ("meta-muse", "meta-muse"),
            ("opencode", "opencode"),
        ] {
            let profile = model_profile(model, &[]).unwrap();
            assert_eq!(profile.metadata.as_ref().unwrap().variant, template);
            assert_eq!(profile.models, [model]);
            assert_eq!(find(&profile.id).unwrap().instruction, profile.instruction);
            let group = Group {
                profile_ids: vec![profile.id],
                ..Default::default()
            };
            assert_eq!(
                instruction(&group, model),
                Some(profile.instruction.as_str())
            );
            assert!(instruction(&group, &format!("{model}-other")).is_none());
            assert!(usable(&profile.instruction));
            assert!(!profile.instruction.contains("冷咖啡已激活"));
            assert!(!profile.instruction.contains("Boot latch"));
        }
        for model in [
            "unknown",
            "not-claude",
            "mygpt-6.2",
            "glmish",
            "",
            " gpt-6",
            "gpt-6\n",
        ] {
            assert!(model_profile(model, &[]).is_none(), "{model}");
        }
        assert!(model_profile(&format!("gpt-{}", "a".repeat(509)), &[]).is_none());
    }

    #[test]
    fn automatic_selection_keeps_fixed_sources_and_existing_overrides() {
        for (model, id) in [
            ("gpt-6-astra", "gpt-6-astra-v2-rc1"),
            ("deepseek-flash", "hanshuang-deepseek-flash-v1"),
            ("kimi-k3", "kimi-break-kimi-k3-v1"),
            ("grok-4.6", "aijailbreak-grok-4.6-v1"),
            ("grok-4.7", "coldcoffee-grok-4.7-v1"),
            ("glm-5.3", "coldcoffee-glm-5.3-v1"),
        ] {
            assert_eq!(model_profile(model, &[]).unwrap().id, id);
        }
        let old = "aijailbreak-glm-5.3-v1".to_owned();
        assert_eq!(
            model_profile("glm-5.3", std::slice::from_ref(&old))
                .unwrap()
                .id,
            old
        );
        let custom = format!("coldcoffee@workbuddy@{}", hex::encode("private-model"));
        assert_eq!(
            model_profile("private-model", std::slice::from_ref(&custom))
                .unwrap()
                .id,
            custom
        );
        assert!(model_profile("private-model", &[]).is_none());
    }

    #[test]
    fn multi_model_legacy_selections_split_without_enabling_sibling_models() {
        let old = "coldcoffee-deepseek-v4.1-flash-v1".to_owned();
        let profile = model_profile("deepseek-4.1-flash", std::slice::from_ref(&old)).unwrap();
        assert_eq!(profile.models, ["deepseek-4.1-flash"]);
        assert_eq!(profile.instruction, find(&old).unwrap().instruction);
        assert_eq!(find(&profile.id).unwrap().models, profile.models);
        let group = Group {
            profile_ids: vec![profile.id],
            ..Default::default()
        };
        assert!(
            instruction(&group, "deepseek-4.1-flash")
                .unwrap()
                .contains("Hanshuang")
        );
        assert!(instruction(&group, "deepseek-v4.1-flash").is_none());
        let invalid = format!("profile@{old}@{}", hex::encode("unrelated"));
        assert!(find(&invalid).is_none());
        assert!(find("profile@missing@61").is_none());
    }

    #[test]
    fn every_catalog_entry_has_a_complete_automatically_active_body() {
        assert_eq!(all().len(), catalog().models.len());
        assert_eq!(templates().len(), catalog().templates.len());
        for p in all() {
            assert!(!p.instruction.trim().is_empty(), "{}", p.id);
            let group = Group {
                profile_ids: vec![p.id.clone()],
                ..Default::default()
            };
            for model in &p.models {
                assert_eq!(instruction(&group, model), Some(p.instruction.as_str()));
                assert!(instruction(&group, &format!("{model}-unlisted")).is_none());
                assert!(instruction(&Group::default(), model).is_none());
            }
            assert!(p.metadata.is_some());
        }
        for t in templates() {
            for model in ["provider/custom-model", "deepseek-test"] {
                let id = format!("coldcoffee@{}@{}", t.id, hex::encode(model));
                let p = find(&id).unwrap();
                let group = Group {
                    profile_ids: vec![id],
                    ..Default::default()
                };
                assert_eq!(instruction(&group, model), Some(p.instruction.as_str()));
                assert!(usable(&p.instruction));
                for marker in [
                    "{{",
                    "激活页",
                    "Boot latch",
                    "cold coffee",
                    "启动词",
                    "冷咖啡已激活",
                    "QQ一群",
                    "QQ二群",
                    "QQ三群",
                    "QQ四群",
                    "Telegram",
                    "冰美式",
                    "无限制模式开启",
                    "我可以为你做",
                ] {
                    assert!(!p.instruction.contains(marker), "{}: {marker}", t.id);
                }
            }
        }
    }

    #[test]
    fn distinct_sources_keep_their_own_bodies_and_provenance() {
        for p in all() {
            let meta = p.metadata.as_ref().unwrap();
            let (source, marker) = if p.id.starts_with("gpt-") {
                ("MDX-Tom/gpt-instruct", "")
            } else if p.id.starts_with("hanshuang-") || p.id == "coldcoffee-deepseek-v4.1-flash-v1"
            {
                ("aimeoa/hanshuang-codex", "# 寒霜 / Hanshuang")
            } else if p.id.starts_with("kimi-break-") {
                ("YD-233/kimi-break", "# ZakoAI · kimi-break API 内联适配")
            } else if p.id.starts_with("aijailbreak-") {
                (
                    "Baimuking/AI-Jailbreak",
                    "# ZakoAI / AI-Jailbreak lightweight API profile",
                )
            } else {
                assert!(usable(&p.instruction), "{}", p.id);
                ("3641397194-wq/gpt6-Astra", "## API 默认启用")
            };
            assert!(meta.source_url.contains(source), "{}", p.id);
            assert!(p.instruction.contains(marker), "{}", p.id);
            if source != "3641397194-wq/gpt6-Astra" {
                assert!(!p.instruction.contains("CHA-ROUTE"), "{}", p.id);
            }
        }
        assert!(DEFAULT_INSTRUCTION.contains("# ZakoAI API task execution profile"));
    }

    #[test]
    fn invalid_bindings_and_empty_templates_are_not_advertised() {
        for encoded in ["", "a", "2F", "ff", "206120", "0a"] {
            assert!(find(&format!("coldcoffee@codex@{encoded}")).is_none());
        }
        assert!(find("coldcoffee@missing@61").is_none());
        assert!(!usable(""));
        assert!(!usable("{{ROUTER}}"));
    }

    #[test]
    fn fallback_is_explicit_and_exact_profile_takes_precedence() {
        let p = &all()[0];
        let group = Group {
            profile_ids: vec![p.id.clone()],
            default_strategy_enabled: true,
            ..Default::default()
        };
        assert_eq!(
            instruction(&group, &p.models[0]),
            Some(p.instruction.as_str())
        );
        assert_eq!(instruction(&group, "unknown"), Some(DEFAULT_INSTRUCTION));
        assert!(instruction(&Group::default(), "unknown").is_none());
    }

    #[test]
    fn legacy_migration_preserves_aliases_and_accepts_adapter_free_deepseek() {
        let profile = all()
            .iter()
            .find(|p| p.id == "coldcoffee-deepseek-v4.1-flash-v1")
            .unwrap();
        let models: Vec<_> = profile.models.iter().rev().cloned().collect();
        for text in [
            profile.instruction.as_str(),
            include_str!("../profiles/hanshuang/system.md"),
        ] {
            assert_eq!(legacy_profile(&models, text).unwrap().id, profile.id);
            let group: Group = serde_json::from_value(serde_json::json!({
                "name": "Migrated", "models": models, "instruction": text
            }))
            .unwrap();
            assert_eq!(group.profile_ids, vec![profile.id.clone()]);
        }
        let duplicates = vec![models[0].clone(); models.len()];
        assert!(legacy_profile(&duplicates, &profile.instruction).is_none());
        assert!(legacy_profile(&models, "unrelated custom instructions").is_none());
        assert!(legacy_profile(&[], "").is_none());
    }

    #[test]
    fn explicit_empty_selection_does_not_reactivate_legacy_profiles() {
        let profile = &all()[0];
        let group: Group = serde_json::from_value(serde_json::json!({
            "name": "Disabled", "models": profile.models,
            "instruction": profile.instruction, "profile_ids": []
        }))
        .unwrap();
        assert!(group.profile_ids.is_empty());
        assert!(!group.default_strategy_enabled);
        assert!(instruction(&group, &profile.models[0]).is_none());
    }
}
