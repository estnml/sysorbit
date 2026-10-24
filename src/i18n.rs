//! Translations, from Fluent files under `i18n/` embedded in the binary.
//!
//! The language follows the desktop's locale (`LANGUAGE`, `LC_ALL`,
//! `LC_MESSAGES`, `LANG`); `SYSORBIT_LANG` overrides it. Messages missing
//! from a translation fall back to English.

use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DesktopLanguageRequester, LanguageLoader};
use intl_pluralrules::{PluralCategory, PluralRuleType, PluralRules};
use rust_embed::RustEmbed;
use unic_langid::LanguageIdentifier;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("the English messages are embedded");
    // Isolation marks would end up as invisible characters in canvas text.
    loader.set_use_isolating(false);
    loader
});

/// A translated message: `fl!("id")` or `fl!("id", name = value)`. Ids and
/// argument names are checked against the English file at compile time.
macro_rules! fl {
    ($id:literal) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $id)
    };
    ($id:literal, $($args:expr),* $(,)?) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $id, $($args),*)
    };
}
pub(crate) use fl;

/// Picks the language from the environment. Call once at startup.
pub fn init() {
    let requested: Vec<LanguageIdentifier> = match std::env::var("SYSORBIT_LANG") {
        Ok(language) => language.parse().into_iter().collect(),
        Err(_) => DesktopLanguageRequester::requested_languages(),
    };
    select(&requested);
}

fn select(languages: &[LanguageIdentifier]) {
    if let Err(error) = i18n_embed::select(&*LOADER, &Localizations, languages) {
        eprintln!("sysorbit: translations unavailable: {error}");
    }
    LOADER.set_use_isolating(false);
}

/// The language messages are shown in.
pub fn language() -> LanguageIdentifier {
    LOADER
        .current_languages()
        .first()
        .cloned()
        .unwrap_or_else(|| LOADER.fallback_language().clone())
}

/// The ordinal plural category of `n` in the current language, as Fluent
/// selectors name it: "one" for 1st, "two" for 2nd, and so on in English.
pub fn ordinal_category(n: u32) -> &'static str {
    let category = PluralRules::create(language(), PluralRuleType::ORDINAL)
        .ok()
        .and_then(|rules| rules.select(n).ok());
    match category {
        Some(PluralCategory::ZERO) => "zero",
        Some(PluralCategory::ONE) => "one",
        Some(PluralCategory::TWO) => "two",
        Some(PluralCategory::FEW) => "few",
        Some(PluralCategory::MANY) => "many",
        _ => "other",
    }
}

/// Uppercases the first letter the way the current language does.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => uppercase(first.encode_utf8(&mut [0; 4])) + chars.as_str(),
        None => String::new(),
    }
}

/// Uppercases the way the current language does: Turkish and Azerbaijani
/// have a dotted capital İ.
pub fn uppercase(text: &str) -> String {
    if matches!(language().language.as_str(), "tr" | "az") {
        text.replace('i', "İ").to_uppercase()
    } else {
        text.to_uppercase()
    }
}

/// Serializes tests that switch the language, and switches it.
#[cfg(test)]
pub fn test_language(language: &str) -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let language: LanguageIdentifier = language.parse().expect("language id");
    select(&[language]);
    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_translation_parses_and_knows_only_english_ids() {
        let english = message_ids("en");
        for file in Localizations::iter() {
            let language = file.split('/').next().unwrap_or_default().to_owned();
            let ids = message_ids(&language);
            let unknown: Vec<&String> = ids.iter().filter(|id| !english.contains(id)).collect();
            assert!(unknown.is_empty(), "{file}: unknown ids {unknown:?}");
        }
    }

    fn message_ids(language: &str) -> Vec<String> {
        let path = format!("{language}/sysorbit.ftl");
        let file = Localizations::get(&path).unwrap_or_else(|| panic!("{path} missing"));
        let source = String::from_utf8(file.data.into_owned()).expect("UTF-8");
        let resource = fluent_syntax::parser::parse(source.as_str())
            .unwrap_or_else(|(_, errors)| panic!("{path}: {errors:?}"));
        resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                fluent_syntax::ast::Entry::Message(message) => Some(message.id.name.to_owned()),
                _ => None,
            })
            .collect()
    }
}
