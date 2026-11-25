//! Source-managed container edition: keep saved model selections usable, without bundled instructions.
use crate::store::Group;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Serialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
    pub model_family: &'static str,
    pub instruction: &'static str,
    pub metadata: Option<()>,
}

#[derive(Serialize)]
pub struct ProfileTemplate {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct Catalog {
    models: BTreeMap<String, Vec<String>>,
    templates: Vec<String>,
}
fn catalog() -> &'static Catalog {
    static DATA: OnceLock<Catalog> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("relay-catalog.json"))
            .expect("invalid relay model metadata")
    })
}

fn descriptor(id: &str, models: Vec<String>) -> Profile {
    let family = [
        "gpt-5.6", "gpt-6.2", "gpt-6.1", "gpt-6", "claude", "grok", "gemini", "deepseek", "glm",
        "kimi",
    ]
    .into_iter()
    .find(|family| {
        models[0] == *family
            || models[0]
                .strip_prefix(family)
                .is_some_and(|s| s.starts_with('-'))
    })
    .unwrap_or("custom");
    Profile {
        id: id.into(),
        name: format!("{} · 透传", models[0]),
        models,
        model_family: family,
        instruction: "",
        metadata: None,
    }
}
pub fn all() -> &'static [Profile] {
    static PROFILES: OnceLock<Vec<Profile>> = OnceLock::new();
    PROFILES.get_or_init(|| {
        catalog()
            .models
            .iter()
            .map(|(id, models)| descriptor(id, models.clone()))
            .collect()
    })
}
pub fn templates() -> &'static [ProfileTemplate] {
    static TEMPLATES: OnceLock<Vec<ProfileTemplate>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        catalog()
            .templates
            .iter()
            .map(|id| ProfileTemplate {
                id: id.clone(),
                name: format!("{id} · 透传"),
            })
            .collect()
    })
}
pub fn find(id: &str) -> Option<Cow<'static, Profile>> {
    if let Some(profile) = all().iter().find(|p| p.id == id) {
        return Some(Cow::Borrowed(profile));
    }
    let (template, encoded) = id.strip_prefix("coldcoffee@")?.split_once('@')?;
    if !catalog().templates.iter().any(|t| t == template)
        || encoded.is_empty()
        || encoded.len() > 1024
    {
        return None;
    }
    let model = String::from_utf8(hex::decode(encoded).ok()?).ok()?;
    if model.trim() != model || model.chars().any(char::is_control) {
        return None;
    }
    Some(Cow::Owned(descriptor(id, vec![model])))
}
pub(crate) fn legacy_profile(_: &[String], _: &str) -> Option<&'static Profile> {
    None
}
pub fn instruction(_: &Group, _: &str) -> Option<&'static str> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_is_compatible_but_never_injects_into_requests() {
        let group = Group {
            profile_ids: all().iter().map(|p| p.id.clone()).collect(),
            default_strategy_enabled: true,
            ..Group::default()
        };
        for p in all() {
            assert!(find(&p.id).is_some());
            for model in &p.models {
                assert!(instruction(&group, model).is_none());
            }
        }
        for t in templates() {
            let id = format!("coldcoffee@{}@{}", t.id, hex::encode("example-model"));
            assert!(find(&id).is_some());
            let selected = Group {
                profile_ids: vec![id],
                ..group.clone()
            };
            assert!(instruction(&selected, "example-model").is_none());
        }
        assert!(instruction(&group, "unknown").is_none());
    }
}
