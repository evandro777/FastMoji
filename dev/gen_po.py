#!/usr/bin/env python3
"""Writes translations/<lang>/LC_MESSAGES/fastmoji.po (bundled by build.rs). Edit the table, run, commit.
Slint's default gettext context is the component name, so msgctxt is `Main` (UI) or `Tr` (strings built in Rust)."""
import os

LANGS = ["pt_BR", "fr", "de", "es", "it", "ja", "zh_CN"]
# (context, msgid, [translation per LANGS])
T = [
("Main", "Search emoji", ["Pesquisar emoji", "Rechercher un émoji", "Emoji suchen", "Buscar emoji", "Cerca emoji", "絵文字を検索", "搜索表情符号"]),
("Main", "Settings", ["Configurações", "Paramètres", "Einstellungen", "Ajustes", "Impostazioni", "設定", "设置"]),
("Main", "Preferences", ["Preferências", "Préférences", "Einstellungen", "Preferencias", "Preferenze", "環境設定", "首选项"]),
("Main", "Skin tone", ["Tom de pele", "Teinte de peau", "Hautfarbe", "Tono de piel", "Tono della pelle", "肌の色", "肤色"]),
("Main", "Emoji size", ["Tamanho do emoji", "Taille des émojis", "Emoji-Größe", "Tamaño del emoji", "Dimensione emoji", "絵文字のサイズ", "表情符号大小"]),
("Main", "Small ({} px)", ["Pequeno ({} px)", "Petit ({} px)", "Klein ({} px)", "Pequeño ({} px)", "Piccolo ({} px)", "小 ({} px)", "小 ({} px)"]),
("Main", "Compact ({} px)", ["Compacto ({} px)", "Compact ({} px)", "Kompakt ({} px)", "Compacto ({} px)", "Compatto ({} px)", "やや小 ({} px)", "紧凑 ({} px)"]),
("Main", "Default ({} px)", ["Padrão ({} px)", "Par défaut ({} px)", "Standard ({} px)", "Predeterminado ({} px)", "Predefinito ({} px)", "標準 ({} px)", "默认 ({} px)"]),
("Main", "Large ({} px)", ["Grande ({} px)", "Grand ({} px)", "Groß ({} px)", "Grande ({} px)", "Grande ({} px)", "大 ({} px)", "大 ({} px)"]),
("Main", "Extra large ({} px)", ["Extra grande ({} px)", "Très grand ({} px)", "Sehr groß ({} px)", "Extra grande ({} px)", "Molto grande ({} px)", "特大 ({} px)", "特大 ({} px)"]),
("Main", "Language", ["Idioma", "Langue", "Sprache", "Idioma", "Lingua", "言語", "语言"]),
("Main", "System default", ["Padrão do sistema", "Défaut du système", "Systemstandard", "Predeterminado del sistema", "Predefinito di sistema", "システムの既定", "系统默认"]),
("Main", "Automatic paste", ["Colar automaticamente", "Collage automatique", "Automatisch einfügen", "Pegar automáticamente", "Incolla automaticamente", "自動で貼り付け", "自动粘贴"]),
("Main", "Keep FastMoji in memory", ["Manter o FastMoji na memória", "Garder FastMoji en mémoire", "FastMoji im Speicher behalten", "Mantener FastMoji en memoria", "Mantieni FastMoji in memoria", "FastMoji をメモリに常駐", "让 FastMoji 常驻内存"]),
("Main", "Shortcut", ["Atalho", "Raccourci", "Tastenkürzel", "Atajo", "Scorciatoia", "ショートカット", "快捷键"]),
("Main", "Always on in this session (the clipboard needs it).", ["Sempre ativo nesta sessão (a área de transferência precisa disso).", "Toujours activé dans cette session (le presse-papiers en a besoin).", "In dieser Sitzung immer an (die Zwischenablage benötigt es).", "Siempre activo en esta sesión (el portapapeles lo necesita).", "Sempre attivo in questa sessione (gli appunti ne hanno bisogno).", "このセッションでは常に有効です（クリップボードに必要）。", "此会话中始终开启（剪贴板需要）。"]),
("Main", "Required for the {} shortcut. Opens instantly and uses a little memory.", ["Necessário para o atalho {}. Abre instantaneamente e usa um pouco de memória.", "Nécessaire pour le raccourci {}. S’ouvre instantanément et utilise un peu de mémoire.", "Erforderlich für das Tastenkürzel {}. Öffnet sofort und braucht etwas Speicher.", "Necesario para el atajo {}. Se abre al instante y usa un poco de memoria.", "Necessario per la scorciatoia {}. Si apre subito e usa un po’ di memoria.", "ショートカット {} に必要です。すぐに開き、少しメモリを使います。", "快捷键 {} 需要此选项。可即时打开，会占用少量内存。"]),
("Main", "Warning: {} is disabled while this is off. Open FastMoji with your own shortcut that runs `fastmoji`.", ["Aviso: {} fica desativado enquanto isto estiver desligado. Abra o FastMoji com um atalho seu que execute `fastmoji`.", "Attention : {} est désactivé tant que cette option est coupée. Ouvrez FastMoji avec votre propre raccourci qui lance `fastmoji`.", "Achtung: {} ist deaktiviert, solange dies aus ist. Öffnen Sie FastMoji mit einem eigenen Tastenkürzel, das `fastmoji` ausführt.", "Aviso: {} queda desactivado mientras esto esté apagado. Abre FastMoji con tu propio atajo que ejecute `fastmoji`.", "Attenzione: {} è disattivato finché questa opzione è spenta. Apri FastMoji con una tua scorciatoia che esegua `fastmoji`.", "注意: これがオフの間は {} は無効です。`fastmoji` を実行する独自のショートカットで FastMoji を開いてください。", "注意：关闭此选项后 {} 将失效。请用自己设置的、运行 `fastmoji` 的快捷键打开 FastMoji。"]),
("Main", "Quit FastMoji", ["Sair do FastMoji", "Quitter FastMoji", "FastMoji beenden", "Salir de FastMoji", "Esci da FastMoji", "FastMoji を終了", "退出 FastMoji"]),
("Main", "About", ["Sobre", "À propos", "Info", "Acerca de", "Informazioni", "情報", "关于"]),
("Main", "Close", ["Fechar", "Fermer", "Schließen", "Cerrar", "Chiudi", "閉じる", "关闭"]),
("Main", "Version {}", ["Versão {}", "Version {}", "Version {}", "Versión {}", "Versione {}", "バージョン {}", "版本 {}"]),
("Main", "Light and fast emoji picker for Linux.", ["Seletor de emojis leve e rápido para Linux.", "Sélecteur d’émojis léger et rapide pour Linux.", "Leichte und schnelle Emoji-Auswahl für Linux.", "Selector de emojis ligero y rápido para Linux.", "Selettore di emoji leggero e veloce per Linux.", "Linux 向けの軽くて高速な絵文字ピッカー。", "轻巧快速的 Linux 表情符号选择器。"]),
("Main", "Author: {}", ["Autor: {}", "Auteur : {}", "Autor: {}", "Autor: {}", "Autore: {}", "作者: {}", "作者：{}"]),
("Main", "License: {}", ["Licença: {}", "Licence : {}", "Lizenz: {}", "Licencia: {}", "Licenza: {}", "ライセンス: {}", "许可证：{}"]),
("Tr", "Recently Used", ["Usados recentemente", "Récemment utilisés", "Zuletzt verwendet", "Usados recientemente", "Usate di recente", "最近使用した絵文字", "最近使用"]),
("Tr", "Smileys & People", ["Carinhas e pessoas", "Smileys et personnes", "Smileys & Personen", "Emoticonos y personas", "Faccine e persone", "顔と人", "表情和人物"]),
("Tr", "Animals & Nature", ["Animais e natureza", "Animaux et nature", "Tiere & Natur", "Animales y naturaleza", "Animali e natura", "動物と自然", "动物和自然"]),
("Tr", "Food & Drink", ["Comida e bebida", "Nourriture et boissons", "Essen & Trinken", "Comida y bebida", "Cibo e bevande", "食べ物と飲み物", "食物和饮料"]),
("Tr", "Activities", ["Atividades", "Activités", "Aktivitäten", "Actividades", "Attività", "アクティビティ", "活动"]),
("Tr", "Travel & Places", ["Viagens e lugares", "Voyages et lieux", "Reisen & Orte", "Viajes y lugares", "Viaggi e luoghi", "旅行と場所", "旅行和地点"]),
("Tr", "Objects", ["Objetos", "Objets", "Objekte", "Objetos", "Oggetti", "物", "物体"]),
("Tr", "Symbols", ["Símbolos", "Symboles", "Symbole", "Símbolos", "Simboli", "記号", "符号"]),
("Tr", "Flags", ["Bandeiras", "Drapeaux", "Flaggen", "Banderas", "Bandiere", "旗", "旗帜"]),
("Tr", "No matches for “{}”", ["Nenhum resultado para “{}”", "Aucun résultat pour « {} »", "Keine Treffer für „{}“", "Sin resultados para “{}”", "Nessun risultato per “{}”", "「{}」に一致するものはありません", "没有与“{}”匹配的结果"]),
("Tr", "Results for “{}”", ["Resultados para “{}”", "Résultats pour « {} »", "Ergebnisse für „{}“", "Resultados para “{}”", "Risultati per “{}”", "「{}」の検索結果", "“{}”的搜索结果"]),
("Tr", "Shortcut unavailable: {}", ["Atalho indisponível: {}", "Raccourci indisponible : {}", "Tastenkürzel nicht verfügbar: {}", "Atajo no disponible: {}", "Scorciatoia non disponibile: {}", "ショートカットを使用できません: {}", "快捷键不可用：{}"]),
("Tr", "Cannot parse {}. Example: Ctrl+Alt+.", ["Não foi possível interpretar {}. Exemplo: Ctrl+Alt+.", "Impossible d’analyser {}. Exemple : Ctrl+Alt+.", "{} kann nicht gelesen werden. Beispiel: Ctrl+Alt+.", "No se puede interpretar {}. Ejemplo: Ctrl+Alt+.", "Impossibile interpretare {}. Esempio: Ctrl+Alt+.", "{} を解釈できません。例: Ctrl+Alt+.", "无法解析 {}。示例：Ctrl+Alt+."]),
]

q = lambda s: '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
for i, lang in enumerate(LANGS):
    d = f"translations/{lang}/LC_MESSAGES"
    os.makedirs(d, exist_ok=True)
    with open(f"{d}/fastmoji.po", "w") as f:
        f.write(f'msgid ""\nmsgstr ""\n"Content-Type: text/plain; charset=UTF-8\\n"\n"Language: {lang}\\n"\n')
        for ctx, msgid, tr in T:
            assert len(tr) == len(LANGS) and tr[i].count("{}") == msgid.count("{}"), msgid
            f.write(f"\nmsgctxt {q(ctx)}\nmsgid {q(msgid)}\nmsgstr {q(tr[i])}\n")
