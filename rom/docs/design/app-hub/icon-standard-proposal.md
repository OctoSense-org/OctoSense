# App icon identity and asset convention

Status: adopted. The preview and launcher now share the host icon catalog;
App Hub owns its canonical icon, and installed apps use their local listing.

## Findings before migration

App Hub's preview uses its own six monochrome SVG glyphs in
`apps/app-hub/resources/icons/`, placed on category-colored backgrounds.
Those were added with the preview UI and differ from the launcher artwork.

The launcher resolves an app ID through `src/octosense/style.rs`:

- News and Maps use `resources/icons/apps/<style>/<id>.svg` in this repository.
- Photos, Sheets and Mail use the Makepad theme catalog under
  `widgets/themes/<style>/icons/<id>.svg` in the pinned sibling framework.
- App Hub uses `resources/icons/apps/apphub.svg`.
- Unknown identities use the framework fallback. Camera has no explicit entry
  in the framework's compiled icon table.

These are host/theme conventions. Built-in apps do not yet share an app-owned
canonical icon directory or a common declaration consumed by every surface.

The Hub repository already defines the publication contract. Its main revision
was checked as `97c2a1fd9aa49a6b87586f228e070e0c16b1067b` during this review:

- `listing.json` declares an `icon` path relative to the bundle.
- PNG and SVG are accepted. Publication requires the named file to exist.
- The documented example uses `assets/icon.svg`; that filename is a convention,
  not a mandatory path in the validator.
- The documentation describes a square icon; the inspected gate checks the
  path, extension and existence, not the decoded image's dimensions.
- The listing parser rejects unknown fields, so theme-specific metadata cannot
  simply be added to the current schema.

Source: [Hub publishing guide](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/97c2a1fd9aa49a6b87586f228e070e0c16b1067b/docs/PUBLISHING.md#the-listing),
[listing validation](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/97c2a1fd9aa49a6b87586f228e070e0c16b1067b/crates/app-policy/src/listing.rs).

## Shared author guidelines

The canonical app-author rules now live in the Hub repository's
[icon guide](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/ICONS.md),
linked from its [publishing contract](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/PUBLISHING.md).
Use [Build your first Hub app](https://github.com/OctoSense-org/OctoSense-App-Hub/blob/main/docs/FIRST-APP.md)
and the [starter](https://github.com/OctoSense-org/OctoSense-App-Hub/tree/main/templates/app)
for new apps. Update shared authoring rules there; this document records the
mobile integration and its design history.

Each app owns the asset named by its listing. Built-in native apps may embed
that source through their build mapping. The host's shared identity resolver
keeps launcher, Recents and Hub surfaces consistent. Existing built-in theme
assets retain their ownership during incremental migration; theme variants
are not additional schema-1 listing fields.

The shared guide distinguishes the Hub's admission checks, the mobile icon
loader's bounds, and recommendations requiring visual review. Passing the
publishing gate alone does not verify native icon rendering.

## Current implementation

- Preview rows and details use the same Makepad `AppIcon` and app IDs as the
  launcher. The duplicate preview glyphs have been removed. Original colors,
  canvas padding and active theme overrides are preserved.
- App Hub declares `assets/icon.svg` in its app-owned `listing.json`. Its build
  script embeds that declared SVG once; the shell's icon catalog and the Hub
  header both resolve the `apphub` identity. Native built-ins consume the icon
  declaration subset only. This file is not a publishable Card listing; Card
  publication still requires the full Hub schema and bundle gate.
- The current design is a shopping bag with the OctoSense logo on its front.
  Its teal tile, ivory bag and mint handle retain the launcher palette while
  clearly identifying an app store. The eight original logo paths are reused
  from `apps/news/resources/icons/octosense.svg`, uniformly scaled and colored
  teal. It replaces the earlier four-tile bag at the user's request to put the
  OctoSense logo inside the bag. Earlier generated explorations remain in
  `icon-exploration/`; the runtime asset is a self-contained native SVG.
- Installed icons are read from their own `listing.json`, with canonical path
  containment, bounded reads, square SVG/PNG checks and a 1024-pixel PNG limit.
  Shell surfaces share that loader and cache artwork until the catalog changes.
  Missing or invalid icons use the existing fallback. Artwork does not bypass
  the separate verified app-launch gate.
- Existing built-in theme assets retain their current location during the
  incremental ownership migration. No framework or sibling app files were
  moved, and the pinned Hub publishing schema is unchanged.
