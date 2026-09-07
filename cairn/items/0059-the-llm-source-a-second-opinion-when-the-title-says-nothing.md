---
id: 59
title: 'The LLM source: a second opinion when the title says nothing'
type: feature
status: backlog
milestone: v0.3.1
depends_on:
- 54
created: 2026-09-06
updated: 2026-09-06
priority: p0
area: naming
effort: l
---

## Problem
Naming reads the agent's terminal title, and that is the right primary source:
the work is already done, it costs nothing, and it needs no key. But it fails in
one specific way — a pane where nothing has published a title has no intent at
all, and the workspace keeps its project name for ever.

namesync's config has a second source for exactly this, disabled by default.

## The framing that keeps this honest
The model is a **source, not a decider**. It supplies a candidate intent from
the pane's viewport, and that candidate then goes through precisely the same
policy every title goes through: junk rejection, the hand-written-name lock, the
debounce, the similarity check, the rate limit. Nothing about *when* a name
changes moves into the model.

That is why this can be added without weakening the thesis the project started
from. namesync's argument was never that models are wrong for this — it was that
generating a name is the part already solved, and deciding when to use one is
the part that is not.

## Cost
Consulted only when the title source yields nothing, and no more often than a
long interval. A naming call per turn per workspace, all day, for a caption, is
not a trade anyone would make deliberately.

## Shape
No HTTP dependency. dirk shells out for the process table, for git and for
notifications; a naming call is the same kind of thing and the same dozen lines
against a crate, a build cost and a transitive tree. Off the drawing thread,
like every other external call.

Provider-agnostic in the config — endpoint, model, key environment variable — so
pointing it at something else is configuration rather than a patch.

## Acceptance criteria
- [ ] Disabled by default, and dirk behaves identically with it off
- [ ] Consulted only when the title source has nothing, and rate limited
- [ ] The candidate goes through the same policy as a title, with no exemptions
- [ ] Never on the drawing thread, and a hung request cannot stall a redraw
- [ ] No API key in the config file — only the name of the variable holding it
- [ ] Absent key, absent curl, or a failed call degrade to the title source
      silently, which is the behaviour without it
- [ ] The viewport sent is bounded, and what is sent is documented
