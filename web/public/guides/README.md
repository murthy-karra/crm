# In-app guides

`create-list.gif` and `create-list-poster.png` are shown by
`src/components/CreateListGuide.vue` when the People page is reached from
"Create a list". They are recorded from the seeded development organization,
never from customer data, and they go stale when the People toolbar or the
Save as list dialog changes.

Regenerate with the dev stack running:

```sh
cd web && GUIDE_PASSWORD="$CRM_DEV_SEED_PASSWORD" pnpm run guide:capture
```

The script (`scripts/capture-create-list-guide.mjs`) launches the installed
Google Chrome through `playwright-core`, records five frames at 1000×625,
assembles them with `ffmpeg`, and deletes the list it created.
