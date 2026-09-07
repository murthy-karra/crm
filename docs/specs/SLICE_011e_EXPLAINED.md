# Slice 011e — Tags

Tags are short labels you put on People: "Investor", "Past client",
"Sphere", "Open house 9/14". Follow Up Boss teams lean on them heavily, and
until now the CRM had no place for them.

**Anyone on the team can tag.** On a Person's page there is a row of tag
chips beside the stage and assignee. Click Add tag, start typing, pick an
existing tag or create a new one on the spot. Click the small x on a chip to
remove it. The People preview shows the same chips, read-only. Spelling and
capitalisation do not create duplicates: typing "investor" when "Investor"
exists just applies the existing tag.

**Renaming and deleting.** A Manage page lists every tag with how many
People carry it. Admins can rename or delete any tag; deleting removes it
from every Person. A member can rename or delete a tag they created
themselves as long as nobody has applied it yet, so a typo is fixed on the
spot without asking. Once a tag is in use, changing or removing it is an
admin action, because it affects every Person and every saved list that uses
the tag. Anyone can still create and apply tags without asking.

**Tags become a filter.** After the second half of this slice, the People
filter bar offers "Tagged" and "Not tagged", each taking one or more tags.
"Tagged: Investor or Sphere" finds People with either; "Not tagged: Do not
call" excludes People carrying that tag, and includes People with no tags at
all. Because saved lists, Today sources and the admin-edited Today rules all
speak the same filter language, a list such as "Investors in Nurture I have
not contacted in 30 days" works and can feed Today with no further change.

**Deleting a tag that a list uses.** The list keeps its name and other
criteria but shows an invalid-filter notice until someone edits it, exactly
as happens today when a list names a stage that no longer exists. Today
rules fall back to their default and say so. The delete confirmation warns
about this. We do not block deletion, because a personal list is private to
its owner (D-046) and an admin could neither see nor fix it.

**Limits.** Up to 200 tags per Organization and 20 tags per Person; names
are 1 to 40 characters. Past those, the app says so rather than accepting
more.

**Why two halves.** The first half is the tag model and the Person page. The
second half is the filter language. Each is small enough to finish and
verify on its own, following the ladder's standing rule. Importing tags from
Follow Up Boss is not part of this slice; this slice builds the place those
tags will land.
