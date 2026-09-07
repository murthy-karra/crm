# Slice 011c — Your saved lists on Today

Choose up to **five saved lists** as sources for your own Today. Use personal
or shared lists; your choices do not change anyone else's Today. Admins still
cannot see another person's private lists.

Turn on **Use as a Today source** from a list, or manage selections on Today.
It uses the saved criteria, not an unsaved preview. “Assigned to me” means the
viewer; without that filter, a source can include unassigned People or People
assigned to colleagues. Matching several lists still produces one Person row,
with each list named as a reason.

Existing unanswered work appears first, then work coming only from lists,
then calls needing an outcome. Within the list-only group, never-contacted
People come first, followed by those contacted longest ago. The date or
“Never contacted” explains that order.

Today still shows at most **200 People**. Existing rules keep their places,
including calls needing an outcome; list-only work uses the remaining space.
Contacting someone removes a list reason only when its criteria stop matching.
“Not contacted in seven days” updates after contact; “Zillow leads” can persist.

If a source cannot load, available work stays visible with a clear notice.
Today cannot claim you are all caught up while work is unavailable. Deleted
lists stop feeding Today. The Operator reports the same queue and limitations;
account/session changes clear private conversation state and reject late replies.

**The revised queries passed an early performance experiment** with 50,000
synthetic People and substantial history. Three small query changes corrected
the original slow results while preserving the compared membership and order.
The busiest case had little spare capacity. The full application still must
pass speed, privacy and recovery tests; the experiment does not establish
production capacity.

**Approved for implementation on 2026-09-06**, after independent review.
Saved-table sorting, editable built-in rules, tags and organization-wide source
assignments stay separate. See [the full specification](SLICE_011c.md).
