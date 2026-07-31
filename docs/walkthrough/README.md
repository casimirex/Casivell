# The walkthrough

`index.html` — a single self-contained page showing what Casivell does, in English.

Open it directly in a browser; it needs no server.

## How it was made, and what that means for trusting it

- **`screens/`** — real captures of the running browser build, taken with headless Chromium
  against a local server. Not mock-ups, and not redrawn.
- **`cli/`** — real terminal output, captured by running each command and redirected to a file.
  Pasted into the page unedited apart from choosing which lines to show.

Regenerating both is a matter of re-running the commands in `cli/` and re-taking the shots; the
figures in the page are therefore only ever as current as the build that produced them.

## A caveat about the narrow-viewport screenshot

Headless Chromium clamps its layout viewport at 485 px, so `06-narrow.png` is the narrowest
width it will actually lay out — not a phone. Below that width the form collapses to a single
column and the wide tables scroll inside their own box, which is verified by the CSS rather
than by a screenshot.
