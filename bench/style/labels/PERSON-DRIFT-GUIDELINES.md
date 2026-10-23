# Person drift guidelines (#354)

You are judging words that a detector flagged as a **slip in grammatical person** in published,
public-domain fiction. Judge each item on its own. Do not look for or use any other labels or the
detector's code.

Each item has an `id`, a `bookPerson` (`first`, `third` or `unknown`: how the book as a whole is
narrated, as far as the item set knows) and a `context`: the flagged sentence and up to about 300
characters each side. The flagged word or name is in `⟦ ⟧`. A context may start or end with `…`
where the window cuts the text.

## What a slip is

A **slip** is a word that puts the narration in the wrong grammatical person by mistake, the kind
of error an editor would mark for correction:

- an *I*, *me*, *my*, *mine* or *myself* in narration that is otherwise third person and should be
  *she*/*he* (*Valancy paused on the porch. I knocked twice.*);
- the narrator called *she*/*he*, or by their own name, in narration that is otherwise first person
  (*I crossed the room. She picked up the knife. I waited.*, where *she* is the narrator).

## Not a slip

Answer `false` when the flagged word is deliberate or is not a person error at all, including:

- an **authorial or narrator intrusion** (*I once said to an old labouring man…*, *as I have
  said*, *my reader*), or a narrator who is a character telling someone else's story (a witness or
  frame narrator: *my poor companion Queequeg was seized with a fever*);
- a **section with a different narrator** (a chapter told by another character in first person,
  where the flagged name or pronoun is the main narrator seen from outside);
- **speech, thought, a letter, a note, a sign or a quotation** the detector took for narration,
  quoted or not;
- an **interjection or fixed phrase** (*my God*, *dear me*, *my dear*);
- a word that is **not the pronoun** (*a coal mine*, a Roman numeral *I*, an initial);
- the narrator's name used **to name or address** (*a girl named Anna*, *Dear Anna*) or the name of
  **another person**;
- anything you cannot tell is an error from the context.

## The label

Answer `true` only when the flagged word is a slip as defined above. Use `note` for a short reason
when you answer `false` or when you were unsure.

## Output

One JSON object per line, in the same order as the items, and nothing else:

```
{"id": "person-drift-001", "label": false, "note": "authorial intrusion"}
{"id": "person-drift-002", "label": true, "note": ""}
```
