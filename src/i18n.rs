use std::{borrow::Cow, collections::HashMap, str::FromStr, sync::RwLock};

use fluent::FluentValue;
use fluent_templates::{LanguageIdentifier, Loader, langid, static_loader};

static_loader! {
    static LOCALES = {
        locales: "assets/lang",
        fallback_language: "en-US",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

static CURRENT: RwLock<Option<LanguageIdentifier>> = RwLock::new(None);

pub fn init(language: Option<&str>) {
    let language = language
        .and_then(|language| LanguageIdentifier::from_str(language).ok())
        .or_else(|| detected().into_iter().next())
        .unwrap_or_else(|| langid!("en-US"));

    *CURRENT.write().unwrap_or_else(|error| error.into_inner()) = Some(language.clone());

    log::info!("language: {language}");
}

pub fn detected() -> Vec<LanguageIdentifier> {
    crate::platform::preferred_languages()
        .into_iter()
        .chain(environment_languages())
        .filter_map(|language| LanguageIdentifier::from_str(&language).ok())
        .collect()
}

pub fn t(key: &'static str) -> String {
    LOCALES.lookup(&current(), key)
}

pub fn t_args(key: &'static str, args: &[(&'static str, FluentValue<'static>)]) -> String {
    let args: HashMap<Cow<'static, str>, FluentValue> = args
        .iter()
        .map(|(name, value)| (Cow::Borrowed(*name), value.clone()))
        .collect();

    LOCALES.lookup_with_args(&current(), key, &args)
}

pub fn locales() -> Vec<LanguageIdentifier> {
    let mut locales: Vec<LanguageIdentifier> = LOCALES.locales().cloned().collect();
    locales.sort();

    locales
}

pub fn name(language: &LanguageIdentifier) -> String {
    LOCALES
        .lookup_single_language(
            language,
            "language-name",
            None::<&HashMap<Cow<'static, str>, FluentValue>>,
        )
        .unwrap_or_else(|_| language.to_string())
}

fn current() -> LanguageIdentifier {
    CURRENT
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .unwrap_or_else(|| langid!("en-US"))
}

fn environment_languages() -> Vec<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| {
            value
                .split(['.', '@'])
                .next()
                .unwrap_or_default()
                .replace('_', "-")
        })
        .filter(|language| !language.is_empty())
        .collect()
}
