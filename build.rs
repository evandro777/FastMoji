//! Compiles the Slint UI and bakes `assets/emojis.csv` into static tables (no runtime CSV parsing).
use std::{fmt::Write as _, path::PathBuf};

#[path = "src/fold.rs"]
mod fold;
use fold::fold;

/// (settings/gettext code, file in assets/i18n). The order is the order of `L10N`.
const LANGS: [(&str, &str); 7] = [
    ("pt_BR", "pt"),
    ("fr", "fr"),
    ("de", "de"),
    ("es", "es"),
    ("it", "it"),
    ("ja", "ja"),
    ("zh_CN", "zh"),
];

const BLOCKLIST: [&str; 3] = ["component", "extras-openmoji", "extras-unicode"];
const CATEGORIES: [&str; 8] = [
    "smileys-people",
    "animals-nature",
    "food-drink",
    "activities",
    "travel-places",
    "objects",
    "symbols",
    "flags",
];

struct Emoji {
    ch: String,
    hex: String,
    name: String,
    terms: String,
    tones: [String; 5],
    has_tones: bool,
    category: String,
}

/// Python's `str.capitalize`: first character upper, the rest lower.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// "2764-FE0F" -> "2764": the emoji-presentation selector is not part of the key in assets/i18n.
fn strip_vs16(hex: &str) -> String {
    hex.split('-').filter(|p| *p != "FE0F").collect::<Vec<_>>().join("-")
}

fn main() {
    println!("cargo:rerun-if-changed=assets/emojis.csv");
    println!("cargo:rerun-if-changed=assets/i18n");
    println!("cargo:rerun-if-changed=translations");
    println!("cargo:rerun-if-changed=ui");

    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_bundled_translations("translations");
    slint_build::compile_with_config("ui/main.slint", config).unwrap();

    let mut reader = csv::Reader::from_path("assets/emojis.csv").unwrap();
    let headers = reader.headers().unwrap().clone();
    let col = |name: &str| headers.iter().position(|h| h == name).unwrap();
    let c_hex = col("hexcode");
    let (c_emoji, c_group, c_annotation, c_tags, c_openmoji) = (
        col("emoji"),
        col("group"),
        col("annotation"),
        col("tags"),
        col("openmoji_tags"),
    );
    let (c_tone, c_combination, c_base) = (col("skintone"), col("skintone_combination"), col("skintone_base_emoji"));

    let mut emojis: Vec<Emoji> = Vec::new();
    for row in reader.records().map(Result::unwrap) {
        let mut category = row[c_group].to_string();
        if BLOCKLIST.contains(&category.as_str()) {
            continue;
        }
        if category == "smileys-emotion" || category == "people-body" {
            category = "smileys-people".into();
        }

        if !row[c_tone].is_empty() {
            // Skin-tone variants are folded into their base emoji (same rule as the Python app).
            if let Some(base) = emojis.iter_mut().find(|e| e.ch == row[c_base] && e.has_tones) {
                if let Ok(i @ 1..=5) = row[c_tone].parse::<usize>() {
                    base.tones[i - 1] = row[c_emoji].to_string();
                }
                continue;
            }
        }

        let name = capitalize(&row[c_annotation]);
        let keywords = [&row[c_tags], &row[c_openmoji]]
            .iter()
            .filter(|f| !f.is_empty())
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut terms = vec![fold(&name)];
        terms.extend(keywords.split(", ").filter(|k| !k.is_empty()).map(fold));
        emojis.push(Emoji {
            ch: row[c_emoji].to_string(),
            hex: strip_vs16(&row[c_hex]),
            name,
            // U+0001 cannot appear in a query, so one `contains` equals "any term contains".
            terms: terms.join("\u{1}"),
            tones: Default::default(),
            has_tones: &row[c_combination] == "single",
            category,
        });
    }

    let mut out = String::new();
    writeln!(out, "pub struct Emoji {{ pub ch: &'static str, pub name: &'static str, pub terms: &'static str, pub tones: [&'static str; 5], pub has_tones: bool }}").unwrap();
    writeln!(out, "pub static EMOJIS: [Emoji; {}] = [", emojis.len()).unwrap();
    for e in &emojis {
        writeln!(
            out,
            "    Emoji {{ ch: {:?}, name: {:?}, terms: {:?}, tones: {:?}, has_tones: {} }},",
            e.ch, e.name, e.terms, e.tones, e.has_tones
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
    // Localized names and search terms, parallel to EMOJIS ("" where CLDR has no entry: English is used).
    writeln!(
        out,
        "pub static L10N: [(&str, [(&str, &str); {}]); {}] = [",
        emojis.len(),
        LANGS.len()
    )
    .unwrap();
    for (code, file) in LANGS {
        let text = std::fs::read_to_string(format!("assets/i18n/{file}.tsv")).unwrap();
        let rows: std::collections::HashMap<&str, (&str, &str)> = text
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\t');
                Some((f.next()?, (f.next()?, f.next().unwrap_or(""))))
            })
            .collect();
        writeln!(out, "    ({code:?}, [").unwrap();
        for e in &emojis {
            let (name, terms) = match rows.get(e.hex.as_str()) {
                Some((label, tags)) => {
                    let mut chars = label.chars();
                    let shown = chars
                        .next()
                        .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect());
                    let mut terms = vec![fold(label)];
                    terms.extend(tags.split(", ").filter(|k| !k.is_empty()).map(fold));
                    (shown, terms.join("\u{1}"))
                }
                None => Default::default(),
            };
            writeln!(out, "        ({name:?}, {terms:?}),").unwrap();
        }
        writeln!(out, "    ]),").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out, "pub static BY_CATEGORY: [&[u16]; {}] = [", CATEGORIES.len()).unwrap();
    for cat in CATEGORIES {
        let ids: Vec<usize> = emojis
            .iter()
            .enumerate()
            .filter(|(_, e)| e.category == cat)
            .map(|(i, _)| i)
            .collect();
        writeln!(out, "    &{ids:?},").unwrap();
    }
    writeln!(out, "];").unwrap();
    std::fs::write(
        PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("emoji_data.rs"),
        out,
    )
    .unwrap();
}
