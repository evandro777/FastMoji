#!/usr/bin/env python3
"""Regenerates assets/i18n/<lang>.tsv (hexcode, name, keywords) from emojibase-data (CLDR annotations).
usage: dev/gen_i18n.py <dir with pt.json fr.json ...>   (download: cdn.jsdelivr.net/npm/emojibase-data@16/<lang>/compact.json)"""
import csv, json, sys

strip = lambda h: "-".join(p for p in h.split("-") if p != "FE0F")
base = {strip(r["hexcode"]) for r in csv.DictReader(open("assets/emojis.csv")) if not r["skintone"]}
for lang in ["pt", "fr", "de", "es", "it", "ja", "zh"]:
    rows = {}
    for e in json.load(open(f"{sys.argv[1]}/{lang}.json")):
        h = strip(e["hexcode"])
        if h in base and h not in rows:
            rows[h] = (e["label"], ", ".join(e.get("tags", [])))
    with open(f"assets/i18n/{lang}.tsv", "w") as f:
        for h, (n, t) in sorted(rows.items()):
            f.write(f"{h}\t{n}\t{t}\n")
    print(lang, len(rows), "of", len(base))
