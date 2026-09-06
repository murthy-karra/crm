# Slice 011b — Saved lists, in plain language

This is a short summary of the slice approved on 2026-09-06. The work is
implemented and checked locally.

**11b lets agents save a set of People filters under a name and use it again
later.** For example, filter for “Assigned to me” and “Stage is Hot Prospect,”
then save it as “My hot prospects.”

When you reopen or refresh the list, it finds the People who match then. If
someone's stage or assignment changes, they can enter or leave the results.
The database does the filtering; the browser receives the matching results.

The everyday flow is:

1. Set your filters on the People page, click **Save as list**, and give it a name.
2. Find it later under the new **Lists** navigation item, in **My lists** or
   **Shared lists**. Each list shows a match count.
3. Open it in the familiar People table, with the same filters and Person preview.

**Personal lists are private to their creator**, including from admins. Each
agent can have 50 personal lists per organization. This privacy covers the
list's name and filters; People remain visible across the organization.

**Shared lists are available to everyone in the organization.** Admins create
and maintain them, up to 200 per organization. Agents can make personal copies
and customize those. The shared and personal limits are separate. In a shared
list, “Assigned to me” means whichever agent opens it.

You can rename a list or adjust its filters, preview the results, then choose
**Save** to keep the changes. **Reset changes** restores the saved version.
**Save as** makes a new list using your current changes; **Duplicate** copies
the saved version. Copies are independent. Deleting a list keeps all its People.

Unsaved changes are clearly marked. The app prompts before discarding them and
flags conflicting edits so one save does not silently overwrite another.

This slice uses the existing filters and keeps the current display limit of
500 People; larger match counts show **500+**. Custom sorting, new search or
filter options, and using these lists on **Today** come later.

The [full specification](SLICE_011b.md) remains the detailed source of truth.
