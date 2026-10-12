#!/usr/bin/env python3
"""Summarize a samply profile (.json.gz with a .syms.json sidecar) or a callgrind output file.

Usage:
  profile_summary.py top PROFILE [-n 40]            self and inclusive tables by function
  profile_summary.py stages PROFILE STAGES.json     inclusive share per named group of functions
  profile_summary.py groups PROFILE REGEX [--root R] inclusive share per capture of REGEX
  profile_summary.py callers PROFILE REGEX          call counts into REGEX by caller (callgrind)

A STAGES.json file is a list of {"label": ..., "regex": ..., "root": optional regex,
"kinds": optional subset of ["samply", "callgrind"]}. Callgrind keeps no full stacks, so "root"
is ignored there and only stages that no other path shares should list it. Inclusive
cost of a group counts every sample (or call edge) once even when matching functions nest.
With "root", only samples whose stack also contains a function matching the root regex count.
"""
import argparse
import bisect
import collections
import gzip
import json
import re
import sys


class SamplyProfile:
    unit = 'samples'

    def __init__(self, path):
        profile = json.load(gzip.open(path))
        syms = json.load(open(path.replace('.json.gz', '.json.syms.json')))
        thread = max(profile['threads'], key=lambda t: t['samples']['length'])
        tables = {}
        for entry in syms['data']:
            table = sorted((s['rva'], s['size'], syms['string_table'][s['symbol']])
                           for s in entry['symbol_table'])
            tables[entry['debug_name']] = ([t[0] for t in table], table)
        func_lib = [thread['resourceTable']['lib'][r] for r in thread['funcTable']['resource']]
        frame_name = []
        for func, address in zip(thread['frameTable']['func'], thread['frameTable']['address']):
            lib = profile['libs'][func_lib[func]]['debugName']
            starts, table = tables.get(lib, ([], []))
            i = bisect.bisect_right(starts, address) - 1
            if i >= 0 and address < table[i][0] + table[i][1]:
                frame_name.append(table[i][2])
            else:
                frame_name.append(f'{lib}+{address:#x}')
        prefix, frames = thread['stackTable']['prefix'], thread['stackTable']['frame']

        self.stacks = []
        for stack in thread['samples']['stack']:
            if stack is not None:
                self.stacks.append(stack_names(stack, prefix, frames, frame_name))
        self.total = len(self.stacks)

    def self_cost(self):
        counts = collections.Counter(stack[0] for stack in self.stacks)
        return counts

    def inclusive(self, regex, root=None):
        pattern = re.compile(regex)
        root_pattern = re.compile(root) if root else None
        count = 0
        for stack in self.stacks:
            if root_pattern and not any(root_pattern.search(n) for n in stack):
                continue
            if any(pattern.search(n) for n in stack):
                count += 1
        return count

    def self_group(self, regex):
        pattern = re.compile(regex)
        return sum(1 for stack in self.stacks if pattern.search(stack[0]))

    def inclusive_groups(self, regex, root=None):
        pattern = re.compile(regex)
        root_pattern = re.compile(root) if root else None
        counts = collections.Counter()
        for stack in self.stacks:
            if root_pattern and not any(root_pattern.search(n) for n in stack):
                continue
            keys = {m.group(1) for n in stack for m in [pattern.search(n)] if m}
            for key in keys:
                counts[key] += 1
        return counts

    def inclusive_all(self):
        counts = collections.Counter()
        for stack in self.stacks:
            for name in set(stack):
                counts[name] += 1
        return counts


def stack_names(index, prefix, frames, frame_name):
    names = []
    while index is not None:
        names.append(frame_name[frames[index]])
        index = prefix[index]
    return names


class CallgrindProfile:
    unit = 'instructions'

    def __init__(self, path):
        names = {}
        self.self_costs = collections.Counter()
        self.edges = collections.Counter()
        self.edge_calls = collections.Counter()
        calls = 0
        current = None
        callee = None
        pending_call = False
        total = 0
        with open(path) as handle:
            for line in handle:
                if line.startswith('fn='):
                    current = self._name(line[3:].rstrip('\n'), names)
                elif line.startswith('cfn='):
                    callee = self._name(line[4:].rstrip('\n'), names)
                elif line.startswith('calls='):
                    pending_call = True
                    calls = int(line[6:].split()[0])
                elif line.startswith('summary:'):
                    total = int(line.split()[1])
                elif line and (line[0].isdigit() or line[0] in '+-*'):
                    parts = line.split()
                    cost = int(parts[1])
                    if pending_call:
                        self.edges[(current, callee)] += cost
                        self.edge_calls[(current, callee)] += calls
                        pending_call = False
                    else:
                        self.self_costs[current] += cost
        self.total = total or sum(self.self_costs.values())
        incoming = collections.defaultdict(list)
        for (caller, callee_name), cost in self.edges.items():
            incoming[callee_name].append((caller, cost))
        self.incoming = incoming

    @staticmethod
    def _name(field, names):
        match = re.match(r'\((\d+)\)(?: (.*))?$', field)
        if not match:
            return field
        key, name = match.groups()
        if name is not None:
            names[key] = name
        return re.sub(r"'\d+$", '', names[key])

    def self_cost(self):
        return self.self_costs

    def inclusive(self, regex, root=None):
        pattern = re.compile(regex)
        root_pattern = re.compile(root) if root else None
        total = 0
        for callee, edges in self.incoming.items():
            if not pattern.search(callee):
                continue
            for caller, cost in edges:
                if pattern.search(caller):
                    continue
                total += cost
        return total

    def self_group(self, regex):
        pattern = re.compile(regex)
        return sum(cost for name, cost in self.self_costs.items() if pattern.search(name))

    def inclusive_groups(self, regex, root=None):
        pattern = re.compile(regex)
        counts = collections.Counter()
        for callee, edges in self.incoming.items():
            match = pattern.search(callee)
            if not match:
                continue
            for caller, cost in edges:
                caller_match = pattern.search(caller)
                if caller_match and caller_match.group(1) == match.group(1):
                    continue
                counts[match.group(1)] += cost
        return counts

    def inclusive_all(self):
        counts = collections.Counter()
        for callee, edges in self.incoming.items():
            for caller, cost in edges:
                if caller != callee and callee != '(below main)':
                    counts[callee] += cost
        return counts


def load(path):
    return SamplyProfile(path) if path.endswith('.json.gz') else CallgrindProfile(path)


def short(name, width=110):
    name = re.sub(r'\s+', ' ', name)
    return name if len(name) <= width else name[:width - 1] + '…'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    top = sub.add_parser('top')
    top.add_argument('profile')
    top.add_argument('-n', type=int, default=40)
    callers = sub.add_parser('callers')
    callers.add_argument('profile')
    callers.add_argument('regex', help='callee functions to attribute (callgrind only)')
    callers.add_argument('-n', type=int, default=20)
    groups = sub.add_parser('groups')
    groups.add_argument('profile')
    groups.add_argument('regex', help='regex with one capture group naming the bucket')
    groups.add_argument('--root')
    groups.add_argument('-n', type=int, default=20)
    stages = sub.add_parser('stages')
    stages.add_argument('profile')
    stages.add_argument('stages')
    args = parser.parse_args()
    profile = load(args.profile)
    total = profile.total
    if args.command == 'top':
        print(f'total {total} {profile.unit}')
        print('\nself')
        for name, cost in profile.self_cost().most_common(args.n):
            print(f'{100 * cost / total:6.2f}%  {short(name)}')
        print('\ninclusive')
        for name, cost in profile.inclusive_all().most_common(args.n):
            print(f'{100 * cost / total:6.2f}%  {short(name)}')
    elif args.command == 'callers':
        pattern = re.compile(args.regex)
        by_caller = collections.Counter()
        for (caller, callee), calls in profile.edge_calls.items():
            if pattern.search(callee):
                by_caller[caller] += calls
        grand = sum(by_caller.values())
        print(f'{grand} calls to functions matching {args.regex!r}')
        for caller, calls in by_caller.most_common(args.n):
            print(f'{100 * calls / grand:6.2f}%  {calls:>10}  {short(caller)}')
    elif args.command == 'groups':
        print(f'total {total} {profile.unit}')
        for key, cost in profile.inclusive_groups(args.regex, args.root).most_common(args.n):
            print(f'{100 * cost / total:6.2f}%  {key}')
    else:
        print(f'total {total} {profile.unit}')
        kind = 'samply' if args.profile.endswith('.json.gz') else 'callgrind'
        for stage in json.load(open(args.stages)):
            if kind not in stage.get('kinds', ['samply', 'callgrind']):
                continue
            cost = profile.inclusive(stage['regex'], stage.get('root'))
            own = profile.self_group(stage['regex'])
            print(f"{stage['label']:<48} incl {100 * cost / total:6.2f}%  self {100 * own / total:6.2f}%")


if __name__ == '__main__':
    sys.exit(main())
