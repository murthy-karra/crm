# Slice 011d — Adjustable Today rules

Today puts People in front of you for three built-in reasons: an inquiry
nobody has answered, a client who replied and is still waiting, and a call
of yours that needs an outcome. Until now those rules were fixed in code.
After this slice, each Organization has its own copy of the three rules,
written in the same language as People filters and saved lists.

**Nothing changes until an admin changes it.** Every Organization starts with
the standard rules, and Today looks exactly as it does now. A database test
proves that before the old code is removed.

**Admins get a "Today rules" page** under Manage. For each rule they can add
or change criteria (for example, exclude a stage, or include unassigned
People), change how many hours count as "fresh" for the two inquiry-and-reply
rules, preview the result for a chosen agent before saving, turn the rule
off, or revert to the default. Two things are locked: the rule's own
defining condition, and "assigned to me" on the inquiry and reply rules, so
an edit cannot flood everyone's Today with the whole Organization's leads.
Turning off the unanswered-inquiry rule asks the admin to type its name.

**Agents can see what changed.** The Manage sources panel on Today lists the
three rules with "Default", "Changed by your admin" or "Off". If a saved rule
stops being valid (say, it names an assignee who has left), Today uses the
default rule and says so, rather than silently dropping work.

**Every change is recorded.** Who changed which rule, when, and to what is
written as a permanent fact.

**New filter chips for everyone.** The three conditions behind the rules —
awaiting a response, client replied and unanswered, a call of mine needs an
outcome — become ordinary filter chips on People and in saved lists. "Awaiting
a response, stage is Hot" is now a saved list you can put on Today yourself.

Priority order (fresh work first, then normal, then list work, then calls
needing an outcome) stays fixed for everyone in this version. Organizations
change which People qualify, not the order of the tiers.

The user chose to deliver this as one larger slice with a backend lane and a
web lane working in parallel, because the three rules share one query and
cannot honestly be split in half. See [the full specification](SLICE_011d.md).
