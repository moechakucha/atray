use font_kit::font::Font as LoadedFont;
use iced::Font;

struct Candidate {
    stem: &'static str,
    aliases: &'static [&'static str],
}

#[cfg(target_os = "macos")]
const CANDIDATES: &[Candidate] = &[
    Candidate {
        stem: "SFNS",
        aliases: &[".SF NS", "System Font", ".AppleSystemUIFont"],
    },
    Candidate {
        stem: "HelveticaNeue",
        aliases: &["Helvetica Neue"],
    },
    Candidate {
        stem: "Helvetica",
        aliases: &["Helvetica"],
    },
];

#[cfg(target_os = "windows")]
const CANDIDATES: &[Candidate] = &[
    Candidate {
        stem: "segoeui",
        aliases: &["Segoe UI"],
    },
    Candidate {
        stem: "arial",
        aliases: &["Arial"],
    },
];

#[cfg(target_os = "linux")]
const CANDIDATES: &[Candidate] = &[
    Candidate {
        stem: "Ubuntu",
        aliases: &["Ubuntu"],
    },
    Candidate {
        stem: "Cantarell",
        aliases: &["Cantarell"],
    },
    Candidate {
        stem: "NotoSans",
        aliases: &["Noto Sans"],
    },
    Candidate {
        stem: "DejaVuSans",
        aliases: &["DejaVu Sans"],
    },
];

pub fn system() -> Option<Font> {
    let active = native_theme_iced::from_system()
        .ok()
        .map(|(_, resolved, _)| native_theme_iced::font_family(&resolved).to_owned());

    log::debug!("native theme font: {active:?}");

    let detected = active.as_deref().and_then(|active| {
        CANDIDATES.iter().find(|candidate| {
            candidate
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(active))
        })
    });

    detected
        .and_then(load)
        .or_else(|| CANDIDATES.iter().find_map(load))
}

fn load(candidate: &Candidate) -> Option<Font> {
    let Some(path) = findfont::find(candidate.stem) else {
        log::debug!("font not installed: {}", candidate.stem);
        return None;
    };

    let loaded = LoadedFont::from_path(&path, 0).ok()?;
    let family: &'static str = Box::leak(loaded.family_name().into_boxed_str());

    log::info!("font: {family} ({})", path.display());

    Some(Font::with_name(family))
}
