#!/bin/sh
# Makes the iOS app icons (gen/apple AppIcon set) from the CaveCAD logo, icons/source.png.
# iOS wants a full square with no transparency and rounds the corners itself, so the logo's own
# rounded corners are filled with its paper colour and its grid lines carried to the edges.
# Needs ImageMagick (brew install imagemagick).
set -e
cd "$(dirname "$0")/../apps/desktop/src-tauri"
set_dir=gen/apple/Assets.xcassets/AppIcon.appiconset
full=$(mktemp -t cavecad-icon).png
magick -size 1024x1024 xc:'#f7f6f2' -fill '#e4e2da' \
  -draw 'rectangle 0,289 1023,294' -draw 'rectangle 0,729 1023,734' \
  -draw 'rectangle 289,0 294,1023' -draw 'rectangle 729,0 734,1023' \
  icons/source.png -composite -alpha remove -alpha off -depth 8 "PNG24:$full"
for f in "$set_dir"/AppIcon-*.png; do
  name=$(basename "$f" .png)            # AppIcon-83.5x83.5@2x-1
  spec=${name#AppIcon-}                 # 83.5x83.5@2x-1
  base=${spec%%x*}; base=${base%%@*}    # 83.5 (or 512)
  scale=$(echo "$spec" | sed -n 's/.*@\([0-9]\)x.*/\1/p')
  px=$(echo "$base * $scale" | bc | cut -d. -f1)
  magick "$full" -resize "${px}x${px}" -depth 8 "PNG24:$f"
  echo "$name ${px}px"
done
rm -f "$full"
