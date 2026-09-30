//! The locale loader (ex-711): upstream's `packages/excalidraw/i18n.ts`,
//! its `languages` list, `setLanguage` and `t`, held to what upstream's own
//! code returns.
//!
//! Fixture: `tests/fixtures/i18n.json`, written by `tools/goldens/i18n.mjs`
//! from the pinned checkout: the threshold, the listed languages and every
//! candidate, the locale files, per language the document's `dir` and
//! `lang` after setLanguage and `t` on sampled paths (with the English
//! fallback, a namespace, missing keys and the `fallback` argument), and
//! `t` with replacements. The locale files under `assets/locales` are the
//! same generator's verbatim copies.

use excali_ui::i18n::{
    english, languages, languages_with, locale_path, percentages, I18n, Language, CANDIDATES,
    COMPLETION_THRESHOLD, DEFAULT_LANG,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/i18n.json")).unwrap()
}

fn language(v: &Value) -> Language {
    Language {
        code: v["code"].as_str().unwrap().to_owned(),
        label: v["label"].as_str().unwrap().to_owned(),
        rtl: v["rtl"].as_bool().unwrap_or(false),
    }
}

fn languages_of(v: &Value) -> Vec<Language> {
    v.as_array().unwrap().iter().map(language).collect()
}

fn locale_file(code: &str) -> Option<String> {
    let path = format!(
        "{}/assets/{}",
        env!("CARGO_MANIFEST_DIR"),
        locale_path(code)
    );
    std::fs::read_to_string(path).ok()
}

/// `String(value)` of a replacement value as upstream's `t` writes it.
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => n.as_f64().unwrap().to_string(),
        },
        other => panic!("replacement {other}"),
    }
}

#[test]
fn threshold_and_default_language() {
    let f = fixture();
    assert_eq!(
        f64::from(COMPLETION_THRESHOLD),
        f["threshold"].as_f64().unwrap()
    );
    assert_eq!(DEFAULT_LANG.to_language(), language(&f["defaultLang"]));
}

#[test]
fn languages_are_the_complete_ones_sorted_by_label() {
    let f = fixture();
    assert_eq!(languages(), languages_of(&f["languages"]));
    assert_eq!(
        languages_with(percentages(), 0),
        languages_of(&f["candidates"])
    );
    assert_eq!(
        CANDIDATES.len() + 1,
        f["candidates"].as_array().unwrap().len()
    );
}

#[test]
fn languages_follow_the_percentages() {
    let mut p = percentages().clone();
    p["de-DE"] = Value::from(84);
    p["hi-IN"] = Value::from(85);
    p.as_object_mut().unwrap().remove("fr-FR");
    let codes: Vec<String> = languages_with(&p, COMPLETION_THRESHOLD)
        .into_iter()
        .map(|l| l.code)
        .collect();
    assert!(!codes.contains(&"de-DE".to_owned()));
    assert!(!codes.contains(&"fr-FR".to_owned()));
    assert!(codes.contains(&"hi-IN".to_owned()));
    assert_eq!(codes[0], "en");
}

#[test]
fn english_has_633_keys_and_58_locale_files_are_vendored() {
    let f = fixture();
    fn leaves(v: &Value) -> usize {
        match v {
            Value::Object(m) => m.values().map(leaves).sum(),
            _ => 1,
        }
    }
    assert_eq!(leaves(english()), 633);
    assert_eq!(f["keys"].as_u64().unwrap(), 633);
    let files = f["files"].as_array().unwrap();
    assert_eq!(files.len(), 58);
    for file in files {
        let code = file.as_str().unwrap().trim_end_matches(".json");
        assert!(locale_file(code).is_some(), "{code}");
    }
}

#[test]
fn set_language_sets_dir_lang_and_translations() {
    let f = fixture();
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 58);
    for case in cases {
        let lang = language(&case["lang"]);
        let mut i18n = I18n::new();
        i18n.set_language(lang.clone(), locale_file(&lang.code).as_deref());
        let code = &lang.code;
        assert_eq!(i18n.language(), &lang);
        assert_eq!(i18n.dir(), case["dir"].as_str().unwrap(), "{code}");
        assert_eq!(
            i18n.document_attributes(),
            [
                ("dir", case["dir"].as_str().unwrap()),
                ("lang", case["htmlLang"].as_str().unwrap())
            ],
            "{code}"
        );
        for (path, text) in case["t"].as_object().unwrap() {
            assert_eq!(i18n.t(path), text.as_str().unwrap(), "{code} {path}");
        }
        for (path, text) in case["fallback"].as_object().unwrap() {
            assert_eq!(
                i18n.t_with(path, &[], Some("fallback text")),
                text.as_str().unwrap(),
                "{code} {path}"
            );
        }
    }
}

#[test]
fn missing_keys_are_none_without_a_fallback() {
    let i18n = I18n::new();
    assert_eq!(i18n.try_t("labels.doesNotExist", &[], None), None);
    assert_eq!(i18n.try_t("labels", &[], None), None);
    assert_eq!(
        i18n.try_t("labels.delete", &[], None).as_deref(),
        Some("Delete")
    );
    assert_eq!(i18n.t("labels.doesNotExist"), "");
}

#[test]
fn invalid_locale_data_falls_back_to_english() {
    let mut i18n = I18n::new();
    let lang = Language {
        code: "de-DE".into(),
        label: "Deutsch".into(),
        rtl: false,
    };
    i18n.set_language(lang, Some("{ not json"));
    assert_eq!(i18n.t("labels.delete"), "Delete");
}

#[test]
fn replacements_fill_the_first_slot_with_js_substitutions() {
    let f = fixture();
    let replacements = f["replacements"].as_array().unwrap();
    assert!(replacements.len() > 40);
    for r in replacements {
        let code = r["code"].as_str().unwrap();
        let lang = languages().into_iter().find(|l| l.code == code).unwrap();
        let mut i18n = I18n::new();
        i18n.set_language(lang, locale_file(code).as_deref());
        let pairs: Vec<(String, String)> = match &r["replacement"] {
            Value::Null => vec![],
            Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), js_string(v))).collect(),
            other => panic!("{other}"),
        };
        let pairs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let path = r["path"].as_str().unwrap();
        assert_eq!(
            i18n.t_with(path, &pairs, None),
            r["result"].as_str().unwrap(),
            "{code} {path} {pairs:?}"
        );
    }
}
