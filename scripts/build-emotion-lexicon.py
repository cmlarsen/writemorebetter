#!/usr/bin/env python3
"""Derive style/data/emotions/emotions.tsv from Princeton WordNet 3.0 and the bundled Lancaster norms.

Usage: scripts/build-emotion-lexicon.py WordNet-3.0.tar.gz [out.tsv]

The input is the release tarball from https://wordnetcode.princeton.edu/3.0/WordNet-3.0.tar.gz,
read in place (nothing is extracted). The Lancaster table is style/data/norms/lancaster-perceptual.tsv,
built by scripts/build-lancaster-norms.py. The derivation is recorded in docs/STYLE-EMOTION-TELLS.md.
"""
import hashlib
import io
import sys
import tarfile
from collections import defaultdict
from pathlib import Path

SHA256 = '640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52'
LANCASTER = Path(__file__).resolve().parent.parent / 'style/data/norms/lancaster-perceptual.tsv'
NOUN_FEELING = 12
VERB_EMOTION = 37
# WordNet verb frame 10, "Something ----s somebody": the stimulus is the subject, so the
# participle names the experiencer's state (frighten → frightened). Love, hate and fear lack it.
STIMULUS_FRAME = '10'

# Lemmas the selection admits whose ordinary reading in fiction is not a named emotion: bodily
# sensations (the showing a writer would use instead), traits and attitudes, stimulus words that
# describe what causes a feeling, and words whose common sense is not the emotional one.
EXCLUDED = set("""
addiction affect aggression aggressive aggressiveness algolagnia algolagnic algophilia ambition
ambitious amiability amicability amicableness amiable anaphrodisia anglophilia anglophilic anglophobia
anglophobic aphrodisia appetence appetency appetent appetite approbation attachment attentive avid
avidity avidness awful bang beatification beneficence beneficent blessed blessedness boot brace
brittle buoyancy buoyant capricious caprice carnal carnality caring cer charge chill close closeness
cold coldhearted coldheartedness comfortable compatibility conceit concern confidence confident
confidential conscience conscientious cool coolness creepy-crawlies dark dead dear dignity
discerning disinclination disinclined distance distant dream dreaminess dreamy dull easiness easy
ego egotism egotistic electric electricity emotion emotional emotionless emotionlessness esteem
fatigue favor favour fetish fever feeling feelings fidget fiery fire fit flush fortunate friendly
friendliness gall gamey gamy gay glow golden good gravity hackles hard hardhearted hardheartedness
harassment heart hearty heavy heaviness hot humble humbleness humility humor humorous humour
hungriness hungry ill impulse inclination inclined indifference indifferent insensible insight
intimacy intoxication itch itchy kick kind leaning light lightness lightsome liking live lividity
low lowness masochism masochistic meek meekness mellow mellowness mercy mood moody near nice
nymphomania nymphomaniacal oppression oppressive pain painful painfulness pall partiality
perceptive perceptiveness perceptivity pet phlegm phlegmatic phlegmatical physical pleasant
pleasantness preference propensity quiet quietness quietude radiance raw regard respect rough
sadism sadistic sadomasochism sadomasochistic satyriasis score scene secure security sensation
sensitive sensitiveness sensitivity sensual sensualism sensuality sensualness sensuous sensuousness
serious seriousness sex sexual sexiness sexy shadow sick sincere sincerity soreness sore soul
spirit spirited stab state stomach stupor submission surrender taste tedious tedium temper
temptation tendency tender tenderness thing tired torture torturous touchy-feely transport twinge
unpleasant unpleasantness urge vain vanity velleity warm warmness warmth warpath weakness weight
weighted weighty whim wild wildness wish wishing worship wound
abase allay ambition appreciate arouse beatify bedevil benight beset bewitch bridle browbeat buffalo
bug bully bullyrag captivate care chevvy chevy chill chivvy chivy cotton cow craze dehumanise
dehumanize demean derange devil disorder distract dither domineer drool dun elicit emote envenom
evoke ferment floor fume fuss galvanise galvanize gentle goad gravel grizzle gruntle hamstring
harass harbor harry hassle hector hero-worship humanise humanize humble impress ingratiate inspire
invite keen lenify lighten lull nark nerve niggle pester pother preoccupy rag recapture saint sex
smoulder steel stew still stultify tease torment torture trance tranquilize tranquillise tranquillize
transfix tribulate unman untune uplift whelm zap
abhorrent abominable achy affable affectional affective afflictive agonal aloof amative amatory
animal anticipant anticipative antipathetical aphrodisiac aphrodisiacal appreciative ardent arrogant
athirst audacious averse bad-tempered barmy bathetic belligerent bristly casual cheerless choleric
comic comical compassionate complacent conceited contemplative cordial crabbed dauntless deadpan
desirous despiteful detestable devil-may-care devoted diffident direful dreaded dreadful
earnest emotive empathetic empathic enjoyable erotic even-tempered expressionless fearless fearsome
fervent fervid fidgety fleshly fussy funny genial godly good-humored good-humoured good-tempered
grave hapless happy-go-lucky harum-scarum heartfelt heartless hesitant high-strung hilarious ho-hum
hokey homicidal horrendous hostile hot-tempered hotheaded ill-tempered impassioned impassive
imperturbable inimical insouciant intrepid irascible irksome jocose jocular jocund jolly jovial
kind-hearted kindhearted kitschy lackadaisical languid languorous lascivious lecherous libidinal
libidinous light-headed lightheaded loathsome lovesome lusty maleficent malevolent malicious maudlin
mawkish meditative merciful merciless misfortunate misogynic misogynistic misogynous murderous
nauseous nervy noisome nonchalant nymphomaniac odious ominous passionate pathetic penitential
penitentiary perfervid pettish piteous pitiless placid playful pleasurable poker-faced pollyannaish
poor potty prickly prurient querulous queasy quick-tempered rancorous reflective remorseless
repugnant repulsive revengeful reverential risible ruminative ruthful ruthless salacious scarey scary
schmaltzy schmalzy scrupulous self-complacent self-conceited self-important self-satisfied
sentimental short-tempered shuddery sober soft-boiled softhearted solemn solicitous soulful spiteful
stolid stonyhearted sultry swollen swollen-headed techy tempestuous tenderhearted terrible thankless
thin-skinned thirsty timid tiresome together torrid torturesome tranquil tyrannical tyrannous unafraid
unassertive unbiased unbiassed unexpressive unflappable unmerciful unsure unthankful vengeful
vindictive warmhearted waspish wearisome well-disposed whimsical whiney whiny zealous zestful
alleviation assuagement belonging cynicism defeatism detachment emulation fervency forgiveness
frisson grievance intertrigo pang languor maleficence malignity mercifulness misanthropy misogamy
misogynism misogyny misology misoneism misopedia mysophilia nausea penance penchant philhellenism
philogyny protectiveness quiver resignation sentiment shiver shudder solace stolidity technophilia
technophobia throes tingle togetherness undertow unassertiveness
""".split())
# Verbs whose participle is usually a cognitive, physical or social state rather than a feeling
# (puzzled, intoxicated, disgraced), or whose passive reports an action (comforted, consoled).
EXCLUDED_VERBS = set("""
acerbate afflict alert alienate allure animate antagonize appease assuage attaint baffle bedaze
befuddle bewilder cark cheer comfort commove conciliate confound console content desert desolate
dignify disgrace dishonor dishonour dislike electrify embolden enliven ennoble estrange flummox forsake
hearten inebriate inspirit interest intoxicate invigorate joy misgive mollify mystify nauseate nonplus
pacify perplex placate puzzle relax sicken soothe spite tempt unhinge unlax unstrain wow
""".split())

# Candidates that name a feeling in ordinary fiction use although their most frequent WordNet sense
# is another one and the Lancaster norms do not rate them dominantly interoceptive. Satellite
# adjectives need this list or that rating.
REVIEWED = set("""
annoyance dislike elation gloom guilt humiliation hysteria impatience irritation misery turmoil
guilty hysterical impatient livid self-conscious sheepish
disgust frustrate relieve surprise
aghast agonised agonized anguished appalled aroused bereaved bereft bitter captivated charmed
cheerful comforted concerned content contented dejected demoralised demoralized desolate desperate
disconsolate discouraged disgruntled disheartened ecstatic elated enraptured forlorn galled gloomy
grief-stricken horror-stricken horror-struck intimidated irate ireful malcontent rapturous reassured
resentful shocked sorry teary terror-stricken terror-struck tormented troubled unnerved wrathful
wretched wroth
""".split())


def read_member(archive, name):
    return archive.extractfile(f'WordNet-3.0/dict/{name}').read().decode('latin-1')


def parse_data(text):
    synsets = {}
    for line in text.splitlines():
        if line.startswith('  '):
            continue
        fields = line.partition(' | ')[0].split()
        count = int(fields[3], 16)
        words = [fields[4 + 2 * i].lower().split('(')[0] for i in range(count)]
        at = 4 + 2 * count
        pointers = []
        for _ in range(int(fields[at])):
            pointers.append(tuple(fields[at + 1:at + 5]))
            at += 4
        frames = set()
        if fields[2] == 'v' and at + 1 < len(fields):
            for i in range(int(fields[at + 1])):
                frames.add(fields[at + 3 + 3 * i])
        synsets[fields[0]] = {'lex': int(fields[1]), 'words': words, 'pointers': pointers, 'frames': frames}
    return synsets


def parse_index(text):
    senses = {}
    for line in text.splitlines():
        if line.startswith('  '):
            continue
        fields = line.split()
        senses[fields[0]] = fields[-int(fields[2]):]
    return senses


def single(word):
    return '_' not in word and word.replace('-', '').isalpha()


def main():
    source = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else 'style/data/emotions/emotions.tsv'
    if REVIEWED & EXCLUDED:
        sys.exit(f'reviewed and excluded: {sorted(REVIEWED & EXCLUDED)}')
    with open(source, 'rb') as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    if digest != SHA256:
        sys.exit(f'unexpected sha256 {digest}')
    with tarfile.open(source, 'r:gz') as archive:
        nouns, adjectives, verbs = (parse_data(read_member(archive, f'data.{pos}')) for pos in ('noun', 'adj', 'verb'))
        noun_index, adjective_index, verb_index = (parse_index(read_member(archive, f'index.{pos}'))
                                                   for pos in ('noun', 'adj', 'verb'))
    interoceptive = set()
    for line in LANCASTER.read_text(encoding='utf-8').splitlines():
        if not line.startswith('#'):
            word, modality, _, _ = line.split('\t')
            interoceptive.add(word) if modality == 'i' else None

    # Nouns: the noun.feeling lexicographer file plus the hyponyms of feeling.n.01 and emotion.n.01.
    emotion_nouns = {offset for offset, synset in nouns.items() if synset['lex'] == NOUN_FEELING}
    pending = [noun_index['feeling'][0], noun_index['emotion'][0]]
    visited = set(pending)
    while pending:
        offset = pending.pop()
        emotion_nouns.add(offset)
        for symbol, target, pos, _ in nouns[offset]['pointers']:
            if symbol in ('~', '~i') and pos == 'n' and target not in visited:
                visited.add(target)
                pending.append(target)

    # Adjectives: derivationally related forms and attributes of those nouns, and the satellites
    # ("similar to") of the adjectives found.
    emotion_adjectives = set()
    for offset in emotion_nouns:
        for symbol, target, pos, _ in nouns[offset]['pointers']:
            if symbol in ('+', '=') and pos in ('a', 's'):
                emotion_adjectives.add(target)
    satellites = set()
    for offset in emotion_adjectives:
        for symbol, target, _, _ in adjectives[offset]['pointers']:
            if symbol == '&' and target not in emotion_adjectives:
                satellites.add(target)

    # Verbs whose participle names a feeling: verb.emotion synsets and verbs derived from an emotion
    # noun (astonish → astonishment), with the stimulus frame.
    derived_verbs = {offset for offset, synset in verbs.items()
                     if any(symbol == '+' and pos == 'n' and target in emotion_nouns
                            for symbol, target, pos, _ in synset['pointers'])}
    emotion_verbs = {offset for offset, synset in verbs.items()
                     if (synset['lex'] == VERB_EMOTION or offset in derived_verbs)
                     and STIMULUS_FRAME in synset['frames']}

    rows = {}
    for kind, synsets, offsets, index in (('n', nouns, emotion_nouns, noun_index),
                                          ('a', adjectives, emotion_adjectives, adjective_index),
                                          ('v', verbs, emotion_verbs, verb_index)):
        for word in sorted({word for offset in offsets for word in synsets[offset]['words']}):
            if (not single(word) or word in EXCLUDED or (kind == 'a' and word.endswith('ing'))
                    or (kind == 'v' and word in EXCLUDED_VERBS)):
                continue
            primary = index.get(word, [None])[0] in offsets
            if primary or word in interoceptive or word in REVIEWED:
                rows.setdefault(word, set()).add(kind)
    for word in sorted({word for offset in satellites for word in adjectives[offset]['words']}):
        if single(word) and word not in EXCLUDED and not word.endswith('ing') and (
                word in interoceptive or word in REVIEWED):
            rows.setdefault(word, set()).add('a')
    for word in sorted({word for offset in derived_verbs for word in verbs[offset]['words']}):
        if word in REVIEWED and single(word) and word not in EXCLUDED_VERBS:
            rows.setdefault(word, set()).add('v')
    unused = sorted(REVIEWED - set(rows))
    if unused:
        sys.exit(f'reviewed words that WordNet does not select: {unused}')

    with open(out, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(HEADER)
        for word in sorted(rows):
            handle.write(f"{word}\t{''.join(sorted(rows[word]))}\n")
    counts = defaultdict(int)
    for kinds in rows.values():
        for kind in kinds:
            counts[kind] += 1
    print(f"{len(rows)} words: {counts['n']} nouns, {counts['a']} adjectives, {counts['v']} verbs")


HEADER = f"""\
# Derived from Princeton WordNet 3.0 (WordNet-3.0.tar.gz, sha256 {SHA256}).
# WordNet 3.0 Copyright 2006 by Princeton University. All rights reserved. Used under the WordNet
# License; the full notice and disclaimer are in style/data/licenses/WordNet-LICENSE. This is a
# modified extract: single-word lemmas of noun.feeling and of the hyponyms of feeling.n.01 and
# emotion.n.01 (n); adjectives derived from or attributes of those nouns and their satellites (a);
# verb.emotion verbs and verbs derived from those nouns, with frame 10, whose participle names a
# feeling (v). A lemma is kept when its
# most frequent WordNet sense is one of those synsets or the Lancaster Sensorimotor Norms rate it
# dominantly interoceptive (style/data/norms/lancaster-perceptual.tsv, CC BY 4.0), minus a reviewed
# exclusion list. Built by scripts/build-emotion-lexicon.py.
"""

if __name__ == '__main__':
    main()
