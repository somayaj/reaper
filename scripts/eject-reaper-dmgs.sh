#!/usr/bin/env bash
# Eject stale Reaper DMG mounts (Reaper, Reaper 1, Reaper 2, …).
set -euo pipefail

found=0
for vol in /Volumes/Reaper /Volumes/Reaper\ [0-9]*; do
  [[ -d "$vol" ]] || continue
  found=1
  echo "Ejecting $vol..."
  hdiutil detach "$vol" -force
done

if [[ "$found" -eq 0 ]]; then
  echo "No Reaper DMG volumes mounted."
else
  echo "Done."
fi
