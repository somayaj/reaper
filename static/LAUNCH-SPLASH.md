# Launch splash

Black screen · **Starting IDE...** · four logo puzzle pieces snap together, then harvest animation runs.

## Preview

```bash
cargo run -- --server
# http://127.0.0.1:<port>/launch-splash-preview.html
```

Pane snap → harvest loop repeats so you can tune timing.

## Edit

| File | What |
|------|------|
| `launch-logo.svg` | Logo split into 4 puzzle pieces (each a clipped slice of the same artwork) |
| `launch-splash-layout.css` | Piece fly-in / roll keyframes |
| `logo-animated.css` | Harvest loop (after assemble) |
| `launch-splash.js` | Triggers `is-assembled` ~1.35s after load |

```bash
./scripts/sync-launch-splash.sh   # after logo edits
```
