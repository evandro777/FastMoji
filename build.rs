//! Compiles the Slint UI and bakes `assets/emojis.csv` into static tables (no runtime CSV parsing).
use std::{fmt::Write as _, path::PathBuf};

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

fn main() {
    println!("cargo:rerun-if-changed=assets/emojis.csv");
    println!("cargo:rerun-if-changed=ui");

    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/main.slint", config).unwrap();

    let mut reader = csv::Reader::from_path("assets/emojis.csv").unwrap();
    let headers = reader.headers().unwrap().clone();
    let col = |name: &str| headers.iter().position(|h| h == name).unwrap();
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
        let mut terms = vec![name.to_lowercase()];
        terms.extend(keywords.split(", ").filter(|k| !k.is_empty()).map(str::to_lowercase));
        emojis.push(Emoji {
            ch: row[c_emoji].to_string(),
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
