# In-app guides

`create-list.mp4` and `create-list-poster.png` are shown by
`src/components/CreateListGuide.vue` when the People page is reached from
"Create a list". They are recorded from the seeded development organization,
never from customer data, and they go stale when the People toolbar or the
Save as list dialog changes.

Regenerate with the dev stack running:

```sh
cd web && GUIDE_PASSWORD="$CRM_DEV_SEED_PASSWORD" pnpm run guide:capture
```

The script (`scripts/capture-create-list-guide.mjs`) launches the installed
Google Chrome through `playwright-core`, captures five still frames at 1120×700
CSS pixels and 2× device scale, assembles them into H.264 with `ffmpeg`
(each frame held about 2.4 s), scales the first frame down for the poster, and
deletes the list it created.
