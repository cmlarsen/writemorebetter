from pathlib import Path
from zipfile import ZipFile
import logging
import re

import lxml.etree as ET

CORE_JAR = Path("libs") / "languagetool-core.jar"
FALSE_FRIENDS_PATH = "org/languagetool/rules/false-friends.xml"
BASE_BUNDLE_PATH = "org/languagetool/MessagesBundle.properties"

BUNDLE_COUNTRY = {"pt": "PT"}


def parse_properties(text):
    props = {}
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i].lstrip()
        i += 1
        if not line or line[0] in "#!":
            continue
        while re.search(r"(?<!\\)(\\\\)*\\$", line) and i < len(lines):
            line = line[:-1] + lines[i].lstrip()
            i += 1
        match = re.match(r"((?:\\.|[^\s=:\\])*)\s*[=:]?\s*(.*)$", line)
        key, value = match.group(1), match.group(2)
        props[unescape_property(key)] = unescape_property(value)
    return props


def unescape_property(value):
    def replace(match):
        escaped = match.group(1)
        if escaped.startswith("u"):
            return chr(int(escaped[1:], 16))
        return {"t": "\t", "n": "\n", "r": "\r", "f": "\f"}.get(escaped, escaped)

    return re.sub(r"\\(u[0-9a-fA-F]{4}|.)", replace, value)


def message_format(pattern, args):
    out = []
    i = 0
    quoted = False
    while i < len(pattern):
        c = pattern[i]
        if c == "'":
            if i + 1 < len(pattern) and pattern[i + 1] == "'":
                out.append("'")
                i += 2
                continue
            quoted = not quoted
            i += 1
        elif c == "{" and not quoted:
            end = pattern.index("}", i)
            out.append(args[int(pattern[i + 1 : end])])
            i = end + 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


class Bundle:
    def __init__(self, *chain):
        self.chain = [props for props in chain if props is not None]

    def get(self, key):
        for props in self.chain:
            if key in props:
                return props[key]
        raise KeyError(key)


def read_bundle(lt_dir, suffix):
    path = lt_dir / "org" / "languagetool" / f"MessagesBundle_{suffix}.properties"
    if path.exists():
        return parse_properties(path.read_text(encoding="utf-8"))
    return None


def read_word_definitions(lt_dir, lang_code):
    path = (
        lt_dir / "org" / "languagetool" / "resource" / lang_code / "word_definitions.txt"
    )
    definitions = {}
    if not path.exists():
        return definitions
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) == 2 and parts[1]:
            definitions[parts[0]] = parts[1]
    return definitions


def token_string(token):
    text = token.text or ""
    if token.get("negate") == "yes":
        text = "!" + text
    if token.get("postag") is not None:
        text += "/" + token.get("postag")
    return text


def same_language(a, b):
    a_lang, _, a_country = a.partition("-")
    b_lang, _, b_country = b.partition("-")
    return a_lang == b_lang and (not a_country or not b_country or a_country == b_country)


def collect_rules(root, lang_code, mother_tongue):
    groups = []
    for group in root.iter("rulegroup"):
        default_off = group.get("default") == "off"
        rules = []
        suggestions = []
        for rule in group.iter("rule"):
            if not default_off:
                default_off = rule.get("default") == "off"
            pattern = rule.find("pattern")
            pattern_lang = pattern.get("lang")
            translations = []
            for translation in rule.iter("translation"):
                translation_lang = translation.get("lang")
                text = translation.text or ""
                if same_language(translation_lang, mother_tongue):
                    translations.append(text)
                if translation_lang == lang_code and same_language(
                    pattern_lang, mother_tongue
                ):
                    if text not in suggestions:
                        suggestions.append(text)
            if pattern_lang == lang_code and translations:
                rules.append((pattern, translations, default_off))
        groups.append((group.get("id"), rules, suggestions))
    return groups


def build_rule_files(lt_dir, lang_code):
    core_jar = ZipFile(lt_dir / CORE_JAR)
    root = ET.fromstring(core_jar.read(FALSE_FRIENDS_PATH))
    base = parse_properties(core_jar.read(BASE_BUNDLE_PATH).decode("utf-8"))
    english = Bundle(base)
    definitions = read_word_definitions(lt_dir, lang_code)

    mother_tongues = set()
    for rule in root.iter("rule"):
        if rule.find("pattern").get("lang") == lang_code:
            for translation in rule.iter("translation"):
                code = translation.get("lang")
                if not same_language(code, lang_code):
                    mother_tongues.add(code)

    files = {}
    for mother_tongue in sorted(mother_tongues):
        own = read_bundle(lt_dir, mother_tongue)
        loader_bundle = Bundle(own, base)
        country = BUNDLE_COUNTRY.get(mother_tongue)
        messages = Bundle(
            read_bundle(lt_dir, f"{mother_tongue}_{country}") if country else own, base
        )

        rules_el = ET.Element("rules", lang=lang_code)
        category = ET.SubElement(
            rules_el,
            "category",
            id="FALSE_FRIENDS",
            name=messages.get("category_false_friend"),
        )

        n_rules = 0
        for group_id, rules, suggestions in collect_rules(root, lang_code, mother_tongue):
            group_el = None
            for pattern, translations, default_off in rules:
                tokens = list(pattern.iter("token"))
                tokens_string = " ".join(token_string(t) for t in tokens).replace("|", "/")
                pattern_text = " ".join(t.text or "" for t in tokens)
                kept = [s for s in suggestions if s.lower() != pattern_text.lower()]
                if not kept:
                    continue

                hint = message_format(
                    loader_bundle.get("false_friend_hint"),
                    [
                        tokens_string,
                        english.get(lang_code),
                        ", ".join(f'"{t}"' for t in translations),
                        english.get(mother_tongue),
                    ],
                )
                description = messages.get("false_friend_desc") + " " + tokens_string

                if group_el is None:
                    group_el = ET.SubElement(
                        category, "rulegroup", id=group_id, name=description
                    )
                rule_el = ET.SubElement(group_el, "rule", name=description)
                if default_off:
                    rule_el.set("default", "off")
                pattern_el = ET.SubElement(rule_el, "pattern")
                for token in tokens:
                    token_el = ET.SubElement(pattern_el, "token")
                    for key, value in token.attrib.items():
                        if key != "{http://www.w3.org/XML/1998/namespace}space":
                            token_el.set(key, value)
                    token_el.text = token.text

                message_el = ET.SubElement(rule_el, "message")
                parts = message_format(loader_bundle.get("false_friend_suggestion"), ["\0"])
                before, _, after = parts.partition("\0")
                message_el.text = hint + " " + before
                previous = None
                for i, suggestion in enumerate(kept):
                    suggestion_el = ET.SubElement(message_el, "suggestion")
                    suggestion_el.text = suggestion
                    tail = ""
                    if suggestion in definitions:
                        tail += f" ({definitions[suggestion]})"
                    if i + 1 < len(kept):
                        tail += ", "
                    suggestion_el.tail = tail
                    previous = suggestion_el
                previous.tail += after

                short_el = ET.SubElement(rule_el, "short")
                short_el.text = messages.get("false_friend")
                n_rules += 1

        files[mother_tongue] = (ET.ElementTree(rules_el), n_rules)
    return files


def write_false_friends(out_dir, lt_dir, lang_code):
    files = build_rule_files(Path(lt_dir), lang_code)
    if not files:
        return

    dest = out_dir / "false_friends"
    dest.mkdir()
    for mother_tongue, (tree, n_rules) in files.items():
        tree.write(
            str(dest / f"{mother_tongue}.xml"),
            encoding="utf-8",
            xml_declaration=True,
            pretty_print=True,
        )
        logging.info(f"Wrote {n_rules} false friend rules for mother tongue {mother_tongue}.")
