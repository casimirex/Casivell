# The walkthrough

`index.html` — a single self-contained page showing what Casivell does, in English.

Open it directly in a browser; it needs no server.

## How it was made, and what that means for trusting it

- **`screens/`** — real captures of the running `web-react` build (`web-react/dist/`), taken
  with headless Chromium against a local server. Not mock-ups, and not redrawn.
- **`cli/`** — real terminal output, captured by running each command and redirected to a file.
  Pasted into the page unedited apart from choosing which lines to show.

Regenerating both is a matter of re-running the commands in `cli/` and re-taking the shots; the
figures in the page are therefore only ever as current as the build that produced them.

## A caveat about the narrow-viewport screenshot

`06-narrow.png` was taken at a 375 px wide viewport (smaller than what the old static page
clamped to). At that size the form collapses to a single column and the tables scroll inside
their own box rather than pushing the page sideways. It is taken from the React build
(`web-react/dist/`) served by a local HTTP server.
