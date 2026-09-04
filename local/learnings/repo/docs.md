# Docs / mkdocs Learnings

## "Deploy docs" GitHub Action fails: broken relative link under strict mode

- The `pages.yml` workflow runs `mkdocs build --strict`, which turns any
  broken-link warning into a hard build failure (exit code 1).
- MkDocs resolves relative links (no leading `/`) relative to the directory
  of the *current* Markdown file, not relative to `docs_dir`. A link written
  as if it were relative to `docs/` will silently resolve to the wrong path
  once the containing file is nested (e.g. under `howto/installation/manual/`).
- Symptom in the Action log:
  `Doc file 'X' contains a link 'Y', but the target '<dir-of-X>/Y' is not
  found among documentation files.` followed by `Aborted with 1 warnings in
  strict mode!`.
- Root cause found for
  `howto/installation/manual/01-prepare-macos-and-fedora-usb.md`: it linked
  to `howto/installation/automatic/index.md` (a docs_dir-root-relative path)
  instead of `../automatic/index.md` (correct relative-to-file path).
- To reproduce/verify locally without pushing: `pip install -r
  requirements-docs.txt && mkdocs build --strict` from the repo root.
- Note: `docs/howto` and `docs/assets` are symlinks to the top-level
  `howto/` and `assets/` directories — this is intentional and works fine
  both locally and in CI (Linux preserves symlinks on checkout); it was
  ruled out as the cause during this investigation.

## Generating a docs page from a data file (feature board)

- `data/features.yml` is the single source of truth. It feeds both
  `docs/features.md` and the `#upstream-work` Discord channel; neither is
  edited by hand any more.
- **The board needs two independent axes, not one.** The first design had a
  single `status` running `wip → submitted → merged`, which was wrong the
  moment audio DSP appeared: it is actively worked on but will never be
  upstreamable, and labelling it "in the works" told users nothing about
  whether they can use it. Now `state` (in-progress / planned / available)
  says what a user gets, and `upstream` (downstream / preparing / submitted /
  merged / revoked / rejected / stale) says what happens to it upstream. A
  merged kernel patch can be invisible to users; a downstream-only feature can
  be the most useful thing in the release.
- A feature that needed several patches gets one row per submission, with
  user-facing titles ("Keyboard backlight after resume") rather than patch
  subjects. That keeps one Discord message per submission without inventing a
  nested `submissions:` list.
- `link` and `authors` are required exactly when `upstream` says something was
  submitted; `help` is a free-text line that drives the "Help wanted" filter.
  It exists because the knowledge of what needs doing sat with one person.
- **One `link` field, not `url` + `docs`.** The first version had two link fields
  that rendered in two different places — the patch URL on the upstream pill, the
  docs page on the title — which made the editor pick a field name before knowing
  where the link would show up. Now a row has at most one `link`, and both the
  title and the upstream pill point at it. The value is either an `https://` URL
  or a page below `docs/` (`post-install/audio-dsp.md`); the renderer tells them
  apart by the prefix and only opens external ones in a new tab (`↗` vs `→`).
  A submitted item must use the URL form, so a relative docs path can never end
  up posted as the link line of a Discord message.
- **Use a native MkDocs `hooks:` entry, not `mkdocs-macros`.** A hook is a
  plain Python file registered in `mkdocs.yml` and needs no extra dependency
  beyond PyYAML. `mkdocs-macros` would run Jinja over *every* page, so any
  future `{{` or `{%` inside a shell/code block (very likely in the install
  docs) would start failing the build.
- MkDocs imports a hook **by file path, not as a package**. Sibling modules
  (`schema.py`, `render_html.py`) are not importable until the hook does
  `sys.path.insert(0, str(Path(__file__).resolve().parent))`.
- The table is emitted as **raw HTML**, because the site only enables
  `attr_list` — there is no way to attach the per-row `data-status` attributes
  the status filter needs from Markdown. Two constraints follow:
  - The HTML block must contain **no blank lines**. Python-Markdown ends a raw
    HTML block at the first blank line and parses the remainder as Markdown.
  - Material only styles `.md-typeset table:not([class])`, so a table *with* a
    class inherits nothing and needs its styling written out in full in
    `docs/stylesheets/extra.css`.
- **`mkdocs serve` does not watch `data/` or `scripts/`.** It only watches
  `docs/` and `mkdocs.yml`, so editing the board data left the preview serving
  a stale page while looking perfectly healthy. Fixed with a `watch:` block in
  `mkdocs.yml`.
- Even with `watch:`, **only data edits reload live**. MkDocs re-imports the
  hook file, but `render_html.py` / `schema.py` stay cached in `sys.modules`,
  so a renderer change needs a `mkdocs serve` restart. Do not try to fix this
  with `importlib.reload()` in `on_config`: it raises
  `ModuleNotFoundError: spec not found for the module 'schema'` under the way
  MkDocs loads hooks, and it takes the whole build down with it, CI included.
- **`display` on a row silently defeats the `hidden` attribute.** The filter hides
  rows with `row.hidden = true`, which relies on the browser rule
  `[hidden] { display: none }` — specificity (0,1,0). The card layout for narrow
  screens sets `.md-typeset .board-table tr { display: block }` at (0,2,1) and
  therefore won out: chips highlighted, counts were right, and every row stayed
  visible on phones while the desktop table filtered correctly. Fixed with an
  explicit `.md-typeset .board-table tr[hidden] { display: none }` at (0,3,1),
  written outside the media query so any future `display` rule on a row cannot
  reintroduce it.
- MkDocs does **not** check links inside raw HTML, so `--strict` cannot catch a
  dead `link:` target. `schema.py` verifies the file exists itself. With
  `use_directory_urls` (the default) the board at `/features/` has to link one
  level up: `post-install/audio-dsp.md` → `../post-install/audio-dsp/`.
- Validation lives in `scripts/ci/schema.py` and raises. Because
  `pages.yml` builds with `--strict`, an unknown status or a duplicate id
  fails the docs build with a pinpointed message instead of deploying a broken
  table. Verified by temporarily setting `status: bogus` — build aborts with
  `items[6] (id: …): 'status' must be one of: …`.
- `pyyaml` was added to `requirements-docs.txt`. It was already present
  transitively via mkdocs, but the hook imports it directly.

## Discord mirror of the upstream work data

- `scripts/ci/sync_discord.py` posts one webhook message per item and
  remembers the returned message id in `data/discord-state.json`
  (committed back by CI). A webhook can edit **its own** messages indefinitely
  via `PATCH /webhooks/{id}/{token}/messages/{message_id}`, which is what makes
  status updates edit the existing post instead of adding a new one.
- Consequence, and the reason the channel had to be re-seeded: a webhook
  **cannot** touch messages posted by a human account. The manually written
  history was not convertible.
- The item `id` is the anchor between the YAML and the Discord message.
  Renaming an id makes the item look new and posts it a second time.
- Messages are **plain content, not embeds** — only plain content makes Discord
  unfurl the lore.kernel.org / GitHub / GitLab link the way the hand-written
  posts did.
- `allowed_mentions: {"parse": []}` is mandatory: the `@handles` in the message
  are GitHub names in plain text and must never turn into real pings.
- The sync is idempotent via a sha256 of the rendered content, so re-running it
  produces no channel noise. A `PATCH` returning 404 (message deleted by hand)
  is recovered by posting again and repairing the state file.
- The state commit-back cannot loop: pushes made with `GITHUB_TOKEN` do not
  trigger workflows.
- **lore.kernel.org is behind Anubis anti-bot protection** — neither WebFetch
  nor `curl` (including the `/raw` endpoint) can read a patch subject from it.
  Patch titles have to be taken from the mailing-list mail itself or from the
  Discord post.

## Second Discord channel: blog posts

A `#blog-updates` channel teases every published post. Same mechanism as the
feature board above, so only the differences are worth writing down.

- `scripts/upstream/` was renamed to `scripts/ci/` when this moved in: the
  directory now holds two unrelated syncs, and both run only in CI. Inside it,
  unprefixed files belong to the feature board and `*_blog_*` ones to the blog.
- The shared half lives in `scripts/ci/discord_webhook.py`: retry/rate-limit
  handling, `post`/`patch`, the state file and the create-or-edit loop. Each
  sync keeps its own webhook secret (`DISCORD_UPSTREAM_WEBHOOK` vs
  `DISCORD_BLOG_WEBHOOK`), its own state file (`data/discord-state.json` vs
  `data/blog-discord-state.json`) and its own concurrency group. Verified the
  refactor by re-running the upstream sync against the live state file:
  `0 posted, 0 updated, 13 unchanged` is the proof it renders byte-identically.
- **Embeds are suppressed here, unlike in `#upstream-work`.** Blog pages carry
  OpenGraph tags, so Discord would unfurl every link into a preview card whose
  description is the very `summary` the message already quotes, above an
  `og:image` that is the same site-wide picture for all posts. Message flag
  `1 << 2` (`SUPPRESS_EMBEDS`) keeps the channel a readable list.
- That flag can only be set when **posting**. `PATCH /webhooks/{id}/{token}/
  messages/{id}` does not accept `flags`, but the flag set at creation survives
  every later edit, so sending it once is enough.
- The teaser is the post's `summary` front matter, not an extract: it is
  hand-written, already whitespace-collapsed by `load_posts()`, and already
  used for the blog card, `<meta name="description">` and the RSS feed. The
  first-paragraph fallback exists only for a post that ever ships without one.
- The sync imports `read_config`/`load_posts` from `website/build.py` instead
  of parsing front matter again, so the announced URL cannot drift from the
  built page. Cost: this job installs the full `website/requirements.txt`,
  not just `pyyaml`. `build.py` is import-safe (`if __name__ == "__main__"`).
- `load_posts()` sorts newest first; the sync reverses it so a backfill lands
  in the channel oldest first and reads chronologically.
- The post `slug` is the anchor between file and message, and it is derived
  from the filename (minus the `YYYY-MM-DD-` prefix) unless the front matter
  overrides it. Renaming a post file therefore posts it a second time —
  set `slug:` explicitly if a file ever has to be renamed.
- Setting `draft: true` on an already-announced post does not delete its
  message; it becomes an orphan, gets a warning and is left alone.

## Replacing MkDocs Material with an own deq.rocks-styled generator

Context: the project owner wanted kait2en.org to look exactly like his other
site, https://deq.rocks (same CSS, only logo and colours swapped), plus a blog.
Material for MkDocs cannot be bent that far without fighting the theme, so the
site is now built by `scripts/site/build.py` (Markdown + Jinja2 + Pygments +
PyYAML, ~500 lines) from `site.yml`, `docs/`, `blog/` and `pages/`.

- **`deq/style.css` is copied verbatim and only re-coloured.** Keeping it a
  literal copy is the point — the brief was "1:1 except logo and colours", and
  a rewrite would drift on the next comparison. All KAIT2EN work sits in an
  appended, clearly marked block at the end of the file.
- **The deq stylesheet references four variables it never declares**
  (`--text`, `--text-muted`, `--bg`, `--card-bg`). Undeclared custom properties
  make the whole declaration invalid at computed-value time, so those rules
  silently do nothing on deq.rocks. They are declared here, which is the
  obvious intent, but it means a handful of blocks (press/downloads/spec
  tables) render slightly differently than on the original site.
- **The brand red needs lifting for text.** The logo red `#d10228` only
  reaches 3.75:1 on black, below 4.5:1 for body text. `--accent` is `#f01f3d`
  (4.8:1); the logo itself keeps the original red because it is a bitmap.
- **The logo is a wide wordmark, not a square icon.** It reads cleanly down to
  ~18px height, so the navbar/footer use the wordmark at `height: 20px` and the
  hero uses it at `width: 520px` in place of deq's icon + `<h1>` lockup. The
  favicons are the 改 glyph alone, cropped out of the PNG and placed on a dark
  rounded square — a transparent white glyph disappears on a light browser tab.
- **The documentation is one page, like deq's `documentation.html`.** One
  Markdown file becomes one `<section id>`; `split: h2` in `site.yml` turns
  each H2 of a file into its own section instead. That is what gives
  `installation.md` the five sidebar entries deq-style granularity needs,
  without splitting the file on disk. Inner headings are demoted one level so
  the section `<h2>` stays the top of its subtree.
- **Every section is a separate Markdown run, so the `toc` extension cannot
  deduplicate heading ids across them.** This is not theoretical: the landing
  page had `id="installation"` twice (the install section and `### Installation`
  in `pages/concept.md`), which silently breaks in-page anchors. Fixed by
  passing a stateful `slugify` that shares one `seen` set per page.
- **Jinja `autoescape=True` escapes pre-rendered HTML.** Rendered Markdown and
  the feature board came out as visible `&lt;p&gt;` text. Wrapping them in
  `markupsafe.Markup` at the source is better than `|safe` in the templates:
  the trust decision stays in the builder, next to where the HTML is produced.
- **`mkdocs build --strict` had to be replaced, not dropped.** `check_links()`
  walks the built HTML, resolves every local `href`/`src`, and verifies both
  the target file and the `#fragment` exist; it also rejects duplicate ids.
  That covers more than `--strict` did — MkDocs never checked links inside raw
  HTML, which is exactly how the feature board is emitted.
- **`render_html.render()` takes a `resolve` callable now.** It used to hardcode
  MkDocs directory URLs (`../post-install/audio-dsp/`); the builder passes a
  resolver that maps a `link:` in `data/features.yml` onto
  `documentation.html#<anchor>` and fails the build if the page is not part of
  the documentation.
- Old MkDocs URLs (`/installation/`, `/post-install/audio-dsp/`, …) are kept
  alive as meta-refresh stubs generated from a `redirects:` map in `site.yml`.
- Firefox headless `--screenshot` only captures the viewport, not the full
  page. To review long pages, build a second copy with
  `.hero { min-height: auto }` appended to its stylesheet and use a very tall
  `--window-size`; `--profile` with a scratch directory is required when a
  normal Firefox is already running.

## Demo deployment on the fork

- `demo.yml` publishes the site to the **fork's own** GitHub Pages under
  `/demo/` and needs no secret: `peaceiris/actions-gh-pages` accepts
  `github_token: ${{ secrets.GITHUB_TOKEN }}` when it pushes to a branch in the
  same repository. Only the cross-repository deploy in `pages.yml` needs the
  `DOCS_DEPLOY_SSH_KEY` deploy key.
- Getting the word "demo" into the URL without creating a second repository:
  build into `public/demo/` and publish `public/` with `force_orphan: true`,
  with a meta-refresh `index.html` at the Pages root. `destination_dir` would
  also work but cannot be combined with `force_orphan`.
- All page links are relative (`{{ base }}`), so the site works unchanged from
  a subdirectory. Only the absolute `site.url` used for canonical/OG tags needs
  `--site-url`; `--noindex` keeps the preview out of search results so it
  cannot compete with kait2en.org for the same content.
- **Pin the production deploy job to the upstream repository.** Without
  `if: github.repository == 'kaiT2en/KaiT2en-Fedora'`, a push to a fork's `main`
  tries to publish kait2en.org with an empty deploy key and fails loudly.
- Enabling Pages on a fork is a one-off API call, because forks do not inherit
  the setting: `POST /repos/{owner}/{repo}/pages` with
  `{"source":{"branch":"gh-pages","path":"/"}}`. The `gh-pages` branch must
  exist first, so run the workflow before enabling Pages. `repo` scope is
  enough; `gh` CLI is not installed on this machine, plain `curl` against the
  REST API works.
- End-to-end latency for a Markdown edit: ~17 s workflow plus ~30 s legacy
  Pages build, so under a minute from `git push` to the live page.
- When diffing built HTML for a code-block change, remember Pygments splits
  tokens with `<span>`s: grepping for `build.py --serve` finds nothing even
  though the text is there. Grep prose, not code.

## Exact logo red, and one consequence

- `--accent` is `#d10228`, the colour straight out of the wordmark. It reaches
  only 3.7:1 against the black background, below the 4.5:1 usually wanted for
  body text; that is a deliberate call by the project owner in favour of an
  exact brand match.
- The consequence is not the link colour but the **filled** accent surfaces.
  deq's accent is a bright green that carries black text (`.btn-primary
  { color: var(--bg-primary) }`), and black on the KAIT2EN red is close to
  unreadable. Anything sitting on an accent background — primary button,
  badges, download buttons — is overridden to white, which reads at 5.6:1.
  Worth checking whenever a palette is swapped: the failure shows up on
  backgrounds, not on text.

## One website folder

- Everything the site is built from lives in `website/`: `site.yml`, `docs/`,
  `blog/`, `pages/`, `templates/`, `static/`, `build.py` and `requirements.txt`,
  with the output in `website/build/`. `build.py` derives `WEBSITE` from
  `__file__` and `ROOT` from its parent, so paths in `site.yml` are relative to
  `website/` and only the feature board reaches back into the repository.
- The feature board deliberately stays in `scripts/ci/` (called
  `scripts/upstream/` until the blog sync moved in next to it), next to the
  `data/features.yml` it renders and the Discord sync that shares its schema.
  `schema.py` only had to learn the new docs path.
- `.gitignore` needs the build output anchored (`/website/build/`). An earlier
  unanchored `site/` rule silently swallowed `scripts/site/`, so `git add`
  refused the builder without an obvious reason.
- Splitting a move from a content change keeps the move reviewable as pure
  renames. `git mv` stages the rename but leaves later edits unstaged, so a
  plain `git commit` captures renames with the *old* content — which produces a
  commit that does not build. Either amend the path fixes into it, or stage the
  old blob at the new path deliberately with
  `git update-index --cacheinfo 100644,<blob>,<new/path>`.

## Cutting the live site over

- The rollback is a *branch*, not a rebuild. `gh-pages` on
  `kaiT2en/kait2en.github.io` keeps the last MkDocs build untouched and the new
  site publishes to `gh-pages-v2`. Going back is then a Pages source change in
  the settings UI, which takes effect in under a minute and needs no working
  build of the old generator — worth remembering, because `mkdocs.yml` and
  `requirements-docs.txt` are deleted and the old site could not be rebuilt
  from `main` any more.
- Because of that split, **merging the PR does not change the live site**. The
  deploy fills the new branch while Pages still serves the old one, so the
  content can be inspected on the branch before anything is switched. This
  removes the usual "merge and hope" moment from a site migration.
- `peaceiris/actions-gh-pages` creates the publish branch if it does not exist,
  including with `force_orphan: true`, so no branch has to be pre-created by
  hand. The existing `DOCS_DEPLOY_SSH_KEY` deploy key covers a new branch on the
  same repository — a key is scoped to the repo, not to a branch.
- The old MkDocs site published `sitemap.xml` (8 URLs), so search engines hold
  those paths. The replacement generator had to grow a sitemap; not emitting one
  would have silently dropped the site's indexing baseline. Static pages get no
  `lastmod` — a build-date stamp on every page claims the whole site changed on
  every deploy, which is worse than no signal at all.
- GitHub Pages cannot do server-side redirects, so the seven legacy URLs are
  meta-refresh pages carrying `<link rel="canonical">`. Search engines treat
  that as a redirect; it is the correct static-hosting answer, but it is a
  client-side redirect and should not be described as a 301.
- `--noindex` has to reach the sitemap too. The demo build writes only a
  `Disallow: /` robots.txt and no sitemap, otherwise the preview deployment
  advertises itself with canonical production URLs.
- A workflow that triggers on one named branch becomes dead the moment that
  branch is merged and deleted, so `demo.yml` was widened to `docs/**`.
  **Superseded:** after go-live the workflow was deleted outright. A fork-only
  workflow (`if: github.repository != ...`) still shows up in the *upstream*
  Actions list, where it can never fire — visible clutter that invites the
  question "why is this here?". A preview deployment is scaffolding for a
  migration; once the thing it previews is live, `build.py --serve` covers the
  need and the workflow should go. Fork PRs keep their safety net either way,
  because the `build` job in `pages.yml` runs on `pull_request`.
- Grepping for the *old* tooling names after the migration is worth it: the
  sample blog post still told readers to `pip install -r requirements-docs.txt`
  and run `scripts/site/build.py`, both of which had ceased to exist. Nothing
  fails a build over prose, so these only surface by looking.
- Cutover done on 2026-08-13: PR #43 filled `gh-pages-v2` without touching the
  live site, the Pages source switch made it live, PR #44 followed. The
  two-step shape held up exactly as intended — the only surprise was found by
  *reading the deployed branch* between the two steps, not by testing after.
- That surprise: the homepage advertised `<link rel="canonical">` of
  `https://kait2en.org/index.html` while the sitemap listed
  `https://kait2en.org/`. Both serve the same bytes, so nothing looked broken;
  the page simply contradicted the sitemap, and search engines would have
  indexed the ugly URL and consolidated inbound links onto it. The generator
  now derives the canonical centrally and maps `index.html` to the bare root,
  with `og:url` reading the same value instead of recomputing it. Any template
  that builds a URL by string-joining `site.url` with a page name will get the
  root wrong; compute it once where the page is written.
- Worth checking after any Pages source switch, all cheap over curl: the CNAME
  survived, `http://` still 301s to `https://`, the `<owner>.github.io` host
  still 301s to the custom domain, the certificate still matches, and `/404.html`
  is still served for unknown paths. All five held here.

## Images in Markdown content

- **Image paths are relative to the output page, not to the source file.**
  `static/img/` is copied to `build/img/`, but posts are written to
  `build/blog/<slug>.html`, so a post needs `../img/x.png` while docs and
  landing-page blocks (both rendered into the output root) need `img/x.png`.
  `check_links()` covers `src=` as well as `href=`, so both the missing `../`
  and a root-absolute `/img/x.png` fail the build with a named file instead of
  shipping a broken image. The root-absolute form fails because
  `page.parent / "/img/x.png"` discards the left side in pathlib and resolves
  against the filesystem root — it would actually work in the browser, which is
  the sort of thing worth knowing before "fixing" the checker to accept it.
- **The lightbox was already there and simply unreachable from Markdown.**
  `base.html` renders `#lightbox` on every page and `main.js` has
  `openLightbox()`, but only hand-written templates called it. One delegated
  `click` listener on `.page-content img, .concept-story img` connects all
  Markdown images at once — no per-image markup and no post-processing pass in
  `build.py`, which is what `wrap_code_blocks()` needs only because a copy
  button has to be injected into the HTML.
- `!img.closest('a')` in that listener matters: an image wrapped in a link must
  follow the link rather than open a viewer over it.
- **There are two content wrappers, not one.** `.page-content` covers the docs
  page, blog posts and the feature board on the index; landing-page prose from
  `website/pages/` renders into `.concept-story`. Styling only the first leaves
  images on the landing page unconstrained by `max-width: 100%`.
- A `width="240"` attribute on an `<img>` survives all of this — verified as 240
  rendered px. `max-width: 100%` only caps, it never stretches.
- **Headless Firefox runs the page's JavaScript, so it can test a click without
  a webdriver.** Build a scratch page that loads `main.js`, calls `.click()` on
  the elements under test, writes the assertions into a `<pre>`, then capture it
  with `firefox --headless --screenshot` and read the PNG. That verified all
  four cases (plain image opens, close resets, linked image does not open,
  `.concept-story` image opens) in one render.
