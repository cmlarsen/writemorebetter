#!/usr/bin/env python3
"""Count <rule> elements in a LanguageTool grammar.xml and classify where they go.

Used by M4D-STRUCTURE to account for every grammar rule. It walks the parsed
tree so it can attribute rules that live inside <rulegroup> containers, which
nlprule flattens (or drops wholesale when the rulegroup itself fails to parse).

Requires `lxml` (in `build/requirements.txt`); the grammar uses entity
references and a DTD that the stdlib parser does not handle.
"""

import argparse
import sys
from collections import Counter

from lxml import etree


def local(tag):
    if not isinstance(tag, str):
        return ""
    return tag.rsplit("}", 1)[-1]


def walk(node, category, group, stats, top_level):
    for child in node:
        name = local(child.tag)
        if not name:
            continue
        if name == "category":
            cat = child.get("id") or child.get("name") or category
            walk(child, cat, None, stats, top_level)
        elif name == "rulegroup":
            gid = child.get("id") or child.get("name")
            rules = [c for c in child if local(c.tag) == "rule"]
            stats["rulegroups"] += 1
            stats["rulegroup_rules"] += len(rules)
            # Fields nlprule's RuleGroup does not understand. `tags` and `type`
            # are attributes; any child other than rule/antipattern/short/url is
            # an unknown field too. A rulegroup that hits one of these fails to
            # deserialize as a whole, so every <rule> inside it is lost at once.
            unknown = []
            for attr in child.attrib:
                if attr not in ("id", "name", "default"):
                    unknown.append("attr:" + attr)
            for c in child:
                cname = local(c.tag)
                if cname and cname not in ("rule", "antipattern", "short", "url"):
                    unknown.append(cname)
            if child.get("tags") is not None:
                stats["rg_tags"] += 1
            if child.get("type") is not None:
                stats["rg_type"] += 1
            if unknown:
                stats["rg_failed_groups"] += 1
                stats["rg_failed_rules"] += len(rules)
                stats["rg_unknown_" + ",".join(sorted(set(unknown)))] += len(rules)
            walk(child, category, gid, stats, top_level)
        elif name == "rule":
            stats["rules"] += 1
            if group:
                stats["rules_in_rulegroup"] += 1
            else:
                stats["rules_toplevel"] += 1
            if child.get("tags") is not None:
                stats["rule_tags"] += 1
            if child.get("type") is not None:
                stats["rule_type"] += 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("grammar")
    args = ap.parse_args()

    parser = etree.XMLParser(resolve_entities=False, load_dtd=False, huge_tree=True)
    tree = etree.parse(args.grammar, parser)
    root = tree.getroot()

    stats = Counter()
    walk(root, None, None, stats, True)

    print(f"total <rule>:          {stats['rules']}")
    print(f"  top-level:           {stats['rules_toplevel']}")
    print(f"  in rulegroups:       {stats['rules_in_rulegroup']}")
    print(f"<rulegroup>:           {stats['rulegroups']}")
    print(f"rulegroups w/ tags:    {stats['rg_tags']}")
    print(f"rulegroups w/ type:    {stats['rg_type']}")
    print(f"failed rulegroups:     {stats['rg_failed_groups']}")
    print(f"rules inside them:     {stats['rg_failed_rules']}")
    print(f"rules tagged picky:    {stats['rule_tags']}")
    print(f"rules w/ type:         {stats['rule_type']}")
    print("--- rulegroup unknown-field rule counts ---")
    for k in sorted(stats):
        if k.startswith("rg_unknown_"):
            print(f"  {k[11:]}: {stats[k]}")


if __name__ == "__main__":
    sys.exit(main())
